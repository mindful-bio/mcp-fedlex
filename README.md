# mcp-fedlex

[![Release](https://img.shields.io/badge/release-v0.2.0-green.svg)](./CHANGELOG.md)
[![MCP](https://img.shields.io/badge/MCP-2025--11--25-blue.svg)](./docs/adr/ADR-008-mcp-protocol-version-upgrade.md)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](./Cargo.toml)

🇬🇧 English · [🇩🇪 Deutsch](./README.de.md) · [🇫🇷 Français](./README.fr.md) · [🇮🇹 Italiano](./README.it.md)


A **Model-Context-Protocol server for Fedlex** (Swiss federal law) — a product of
[mindful.bio](https://mindful.bio). It gives an LLM **citable** access to
consolidated federal law instead of letting it free-form its answers:

> 📖 **Full project description** (five languages: tools, quickstart,
> architecture): **[mcp-fedlex.ch](https://mcp-fedlex.ch)**.
> Live application platform built on top of this server: **[ansv.ch](https://ansv.ch)**.
> See also the [Ecosystem](#ecosystem) section below.

- 📌 **Provenance by construction** — every answer carries its `eli` and the
  `valid_as_of` point-in-time date. The date is **stamped server-side** and
  cannot be tampered with by any tool argument. Structurally distinguished: a
  **norm citation** (`kind: "norm"`) vs. a **discovery hint** (`kind: "hint"`, a
  candidate — *not* a citation), so a reasoner never accidentally records a hit
  as a cited norm.
- 🔒 **Least-privilege RBAC** — 25 tools across four active pools, filtered by
  role (Reader ⊆ Navigator ⊆ Validator). Identity always comes from the verified
  credential, never from an LLM parameter.
- 🧯 **Tenant isolation & quota** — enforced server-side per token (distributed,
  fail-closed token bucket over Redis); live discovery weighs more heavily in the
  quota than local navigation, to protect the public Fedlex endpoint.
- 🧾 **Audit log per call** — every `tools/call` line records tenant, session,
  tool, ELID and point-in-time date; raw arguments and response content are
  fail-closed redacted (PII scrubber, ADR-001).
- 🦀 **Rust, no network in tests** — unit and integration tests run offline;
  live conformance against Fedlex is separate (`-- --ignored`).

> **Note:** This GitHub repository is a **public mirror**. The source of truth
> (CI/CD, releases) lives on a self-hosted GitLab; issues and PRs here are
> triaged but processed there.

## What it can do

25 tools across four active pools, RBAC-filtered (Reader ⊆ Navigator ⊆ Validator).
The **Reader** sees only `LocalNavigation`; **Navigator** (how ansV runs) also gets
`Discovery` and `JoluxMetadata`; **Validator** additionally gets `Validation`.

**Navigation within the act text (AKN, pool `LocalNavigation`, 11 tools)**
`read_article` · `read_element` · `read_document` · `get_structure` · `search_text`
· `get_metadata` · `get_references` · `get_modifications` · `list_components`
· `extract_tables` · `detect_foreign_content`.

**Discovering acts (pool `Discovery`, 3 tools)**
`search_law` · `resolve_sr_number` · `find_related_topic`. Hits carry
**hint provenance** (`kind: "hint"`) — candidates, not norm citations.

**Metadata & relationships (JOLux, pool `JoluxMetadata`, 10 tools)**
`check_in_force` · `list_versions` · `resolve_consolidation_at` · `get_impacts` ·
`get_outgoing_impacts` · `get_article_history` · `get_citations` · `get_taxonomy` ·
`get_subdivisions` · `list_annexes`.

**Validation (pool `Validation`, 1 tool)**
`compare_versions` (version comparison, Validator only).

> `Discovery` and `JoluxMetadata` go **live** to the public Fedlex SPARQL endpoint
> and weigh more heavily in the quota (cost weight 5 instead of 1), whereas
> `LocalNavigation` is served from the pod's manifestation cache.

## Up and running locally in 2 minutes

Prerequisite: Docker with Compose. No Rust toolchain needed.

```bash
cp .env.example .env          # set dev token & co. (defaults are fine for testing)
docker compose up --build     # bring up Reader + Redis
```

The Reader then listens on `http://localhost:8080`. Check health:

```bash
curl -s http://localhost:8080/livez      # -> "ok" (liveness)
curl -s http://localhost:8080/readyz      # -> checks Redis + Fedlex SPARQL
```

List tools (dev token from your `.env`):

```bash
TOKEN=dev-secret-change-me
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq
```

Read an article of the Federal Constitution as of a given date:

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{
          "name":"read_article",
          "arguments":{"eli":"eli/cc/1999/404","eid":"art_1"},
          "as_of":"2024-01-01"
        }
      }' | jq
```

The response contains the norm text **and** a `provenance` block (`eli`, `valid_as_of`).
The optional `as_of` parameter (ISO `YYYY-MM-DD`) controls the point-in-time date;
without it, today applies.

## Connecting to an MCP client

The server speaks MCP over JSON-RPC (protocol `2025-11-25`; a legacy client
explicitly requesting `2024-11-05` still gets `2024-11-05`). There are three
HTTP routes:

- **`POST /mcp`** — the **Streamable HTTP endpoint** of the target revision
  `2025-11-25` (recommended). Enforces two transport guards *before* any work:
  a foreign `Origin` header is rejected with **403** (DNS rebinding protection),
  a set but unsupported `MCP-Protocol-Version` header with **400**.
- **`POST /rpc`** — the legacy endpoint (same `McpService` chain without the two
  guards). Kept for handshake-less legacy clients.
- **`GET /sse`** — opens the SSE stream and announces `/rpc` as the POST address.

Methods: `initialize` (handshake with version negotiation), `tools/list`
(RBAC-filtered), `tools/call` (quota-throttled, through the provenance gate),
`ping` (keep-alive), plus the `notifications/initialized` notification
(acknowledged with **202 Accepted** and no body). **Auth:** Bearer token in the
`Authorization` header on **every** request (except notifications).

`initialize` example:

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize"}' | jq
# -> { "protocolVersion":"2025-11-25",
#      "serverInfo":{"name":"mcp-fedlex-reader", ...},
#      "capabilities":{"tools":{}} }
```

> Without `protocolVersion` in the request, the server negotiates the default
> revision `2025-11-25`. A client that explicitly sends
> `"protocolVersion":"2024-11-05"` still gets `2024-11-05` (backward
> compatibility for legacy clients).

For clients with JSON configuration (e.g. Claude Desktop via an SSE/HTTP bridge
connector), the base URL `http://localhost:8080` and the Bearer token suffice.

### Inspecting it in the browser (MCP Inspector)

To click through the tools interactively, use the official
[MCP Inspector](https://github.com/modelcontextprotocol/inspector). A ready-made
[`inspector.json`](./inspector.json) ships with this repo, so a single command
connects with URL **and** token pre-filled — no manual UI fiddling:

```bash
npx -y @modelcontextprotocol/inspector --config inspector.json --server fedlex
```

The Inspector opens in your browser already connected; pick the **Tools** tab,
e.g. `read_article` with `eli = eli/cc/1999/404` and `eid = art_1`.

`inspector.json` points at `http://localhost:8090/mcp` because the host port is
overridable — if `8080` is already taken, start the server with a different one:

```bash
MCP_HOST_PORT=8090 docker compose up --build   # in-container port stays 8080
```

The container always listens on `8080`; `MCP_HOST_PORT` only remaps the host side
(see `docker-compose.yml`). Keep the value in `inspector.json` in sync with it.

> Configuring it by hand instead? Transport **Streamable HTTP**, URL
> `http://localhost:8090/mcp`, and an enabled `Authorization: Bearer <token>`
> header. The token defaults to `dev-secret-change-me` (your `.env`).

## Configuration

All configuration runs through environment variables. Full reference with defaults
and required fields: **[`docs/70_CONFIG.md`](./docs/70_CONFIG.md)**. The role and
token model (dev token vs. JWT/JWKS) is in
**[`docs/90_AUTH_AND_ROLES.md`](./docs/90_AUTH_AND_ROLES.md)**.

> The Compose setup is meant for **development** (plaintext Redis, static dev
> token). Production operation on Kubernetes (JWT/JWKS, Redis mTLS, SealedSecrets):
> **[`docs/80_DEPLOY.md`](./docs/80_DEPLOY.md)**.

## Pulling a versioned image

For third-party use there are **citable SemVer images** (immutable, bound to a
Git tag) in addition to the rolling tags of the internal continuous deploy:

| Tag | Purpose | Stability |
|-----|---------|-----------|
| `:v0.2.0` | citable release (bound to Git tag `v0.2.0`, MCP `2025-11-25`) | immutable — **recommended for third parties** |
| `:v0.1.0` | older release (bound to Git tag `v0.1.0`, MCP `2024-11-05`) | immutable |
| `:latest` | latest `main` state | rolling |
| `:<short-sha>` | exact commit | immutable, internal |

```bash
docker pull registry.mindful-server.com/mindful-bio/mcp-fedlex:v0.2.0
```

Releases are documented in [`CHANGELOG.md`](./CHANGELOG.md); the reported
`serverInfo.version` (see `initialize`) matches the SemVer from `Cargo.toml`.
A new release is created by a Git tag `vX.Y.Z` — CI automatically builds the
identically named image from it.

## Building & testing from source

```bash
cargo build --workspace
cargo test  --workspace                 # unit/integration tests, no network
cargo test  --workspace -- --ignored      # live conformance against Fedlex (network)
```

## Architecture & decisions

- LikeC4 architecture plan: [`likec4/`](./likec4)
- Capability lexicon (JOLux function space): [`docs/10_LEXICON_jolux.md`](./docs/10_LEXICON_jolux.md)
- Implementation plan & checklist: [`docs/30_PLAN.md`](./docs/30_PLAN.md)
- Open items & usability: [`docs/60_OPEN_ITEMS_AND_USABILITY.md`](./docs/60_OPEN_ITEMS_AND_USABILITY.md)
- Review findings (living register): [`docs/65_REVIEW_FINDINGS.md`](./docs/65_REVIEW_FINDINGS.md)
- Architecture Decision Records: [`docs/adr/`](./docs/adr)
- Contributing: [`CONTRIBUTING.md`](./CONTRIBUTING.md) · Security: [`SECURITY.md`](./SECURITY.md)

## Ecosystem

`mcp-fedlex` is the provenance-guaranteed data layer of a small product family by
[mindful.bio](https://mindful.bio):

| Project | What it is | Link |
|---------|-----------|------|
| **mcp-fedlex** (this repo) | The MCP server: citable, point-in-time-accurate access to Swiss federal law. Full, five-language project description (tools, quickstart, architecture). | **[mcp-fedlex.ch](https://mcp-fedlex.ch)** |
| **ansV** | The **application platform** that uses this server as a Navigator client — legal analyses with a traceable chain of evidence. | **[ansv.ch](https://ansv.ch)** |
| **mindful.bio** | The company behind both projects. | **[mindful.bio](https://mindful.bio)** |

## License

[Apache-2.0](./LICENSE) © mindful.bio

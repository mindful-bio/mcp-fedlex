# mcp-fedlex

[![Release](https://img.shields.io/badge/release-v0.2.0-green.svg)](./CHANGELOG.md)
[![MCP](https://img.shields.io/badge/MCP-2025--11--25-blue.svg)](./docs/adr/ADR-008-mcp-protocol-version-upgrade.md)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](./Cargo.toml)

Ein **Model-Context-Protocol-Server für Fedlex** (Schweizer Bundesrecht) — ein
mindful.bio-Produkt. Er gibt einem LLM **belegbaren** Zugriff auf konsolidiertes
Bundesrecht, statt es frei formulieren zu lassen:

- 📌 **Provenance per Konstruktion** — jede Antwort trägt ihren `eli` und den
  `valid_as_of`-Stichtag. Der Stichtag wird **serverseitig gestempelt** und kann
  von keinem Tool-Argument verfälscht werden.
- 🔒 **Least-Privilege-RBAC** — 22 Werkzeuge in drei Pools, nach Rolle gefiltert
  (Reader ⊆ Navigator ⊆ Validator).
- 🧯 **Mandantentrennung & Quota** — pro Token serverseitig durchgesetzt
  (verteiltes Token-Bucket über Redis).
- 🦀 **Rust, kein Netz im Test** — Unit-/Integrationstests laufen offline;
  Live-Konformanz gegen Fedlex ist separat (`-- --ignored`).

> **Hinweis:** Dieses GitHub-Repository ist ein **öffentlicher Spiegel**. Die
> Quelle der Wahrheit (CI/CD, Releases) liegt auf einer selbst-gehosteten GitLab;
> Issues und PRs hier werden gesichtet, aber dort verarbeitet.

## Was er kann

22 Werkzeuge in drei Pools, RBAC-gefiltert (Reader ⊆ Navigator ⊆ Validator):

**Navigation im Erlasstext (AKN, Pool LocalNavigation)**
`read_article` · `read_element` · `get_structure` · `search_text` · `get_metadata`
· `read_document` · `get_references` · `get_modifications` — plus `compare_versions`
(Versionsvergleich, Pool Validation).

**Auffinden von Erlassen (Discovery)**
`search_law` · `resolve_sr_number` · `find_related_topic`.

**Metadaten & Beziehungen (JOLux, Pool JoluxMetadata)**
`check_in_force` · `list_versions` · `resolve_consolidation_at` · `get_impacts` ·
`get_outgoing_impacts` · `get_article_history` · `get_citations` · `get_taxonomy` ·
`get_subdivisions` · `list_annexes`.

## In 2 Minuten lokal

Voraussetzung: Docker mit Compose. Keine Rust-Toolchain nötig.

```bash
cp .env.example .env          # Dev-Token & Co. setzen (Defaults reichen zum Testen)
docker compose up --build     # Reader + Redis hochfahren
```

Der Reader lauscht dann auf `http://localhost:8080`. Health prüfen:

```bash
curl -s http://localhost:8080/livez      # -> "ok" (Liveness)
curl -s http://localhost:8080/readyz      # -> prüft Redis + Fedlex-SPARQL
```

Werkzeuge auflisten (Dev-Token aus deiner `.env`):

```bash
TOKEN=dev-secret-change-me
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq
```

Einen Artikel der Bundesverfassung zum Stichtag lesen:

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

Die Antwort enthält den Normtext **und** einen `provenance`-Block (`eli`, `valid_as_of`).
Der optionale `as_of`-Parameter (ISO `YYYY-MM-DD`) steuert den Stichtag; ohne ihn gilt
heute.

## An einen MCP-Client anbinden

Der Server spricht MCP über SSE/JSON-RPC (Protokoll `2025-11-25`; ein explizit
`2024-11-05` anfragender Alt-Client erhält weiterhin `2024-11-05`):

- **SSE-Strom:** `GET /sse` — liefert die POST-Adresse für Nachrichten.
- **JSON-RPC:** `POST /rpc` — Methoden `initialize`, `tools/list`, `tools/call`.
- **Auth:** Bearer-Token im `Authorization`-Header bei **jeder** Anfrage.

Beispiel `initialize`:

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize"}' | jq
# -> { "protocolVersion":"2025-11-25",
#      "serverInfo":{"name":"mcp-fedlex-reader", ...},
#      "capabilities":{"tools":{}} }
```

> Ohne `protocolVersion` im Request handelt der Server die Default-Revision
> `2025-11-25` aus. Ein Client, der explizit `"protocolVersion":"2024-11-05"`
> sendet, erhält weiterhin `2024-11-05` (Rückwärtskompatibilität für Alt-Clients).

Für Clients mit JSON-Konfiguration (z. B. Claude Desktop über einen SSE/HTTP-Bridge-
Connector) genügen Basis-URL `http://localhost:8080` und das Bearer-Token.

## Konfiguration

Die gesamte Konfiguration läuft über Umgebungsvariablen. Vollständige Referenz mit
Defaults und Pflichtangaben: **[`docs/70_CONFIG.md`](./docs/70_CONFIG.md)**. Das
Rollen- und Token-Modell (Dev-Token vs. JWT/JWKS) steht in
**[`docs/90_AUTH_AND_ROLES.md`](./docs/90_AUTH_AND_ROLES.md)**.

> Das Compose-Setup ist für **Entwicklung** gedacht (Klartext-Redis, statisches
> Dev-Token). Produktiver Betrieb auf Kubernetes (JWT/JWKS, Redis-mTLS, SealedSecrets):
> **[`docs/80_DEPLOY.md`](./docs/80_DEPLOY.md)**.

## Versioniertes Image beziehen

Für Fremdnutzung gibt es **zitierbare SemVer-Images** (unveränderlich, an einen
Git-Tag gebunden) zusätzlich zu den rollenden Tags des internen Continuous-Deploy:

| Tag | Zweck | Stabilität |
|-----|-------|-----------|
| `:v0.2.0` | zitierbares Release (an Git-Tag `v0.2.0`, MCP `2025-11-25`) | unveränderlich — **für Fremdnutzer empfohlen** |
| `:v0.1.0` | älteres Release (an Git-Tag `v0.1.0`, MCP `2024-11-05`) | unveränderlich |
| `:latest` | jeweils letzter `main`-Stand | rollend |
| `:<short-sha>` | exakter Commit | unveränderlich, intern |

```bash
docker pull registry.mindful-server.com/mindful-bio/mcp-fedlex:v0.2.0
```

Releases sind in [`CHANGELOG.md`](./CHANGELOG.md) dokumentiert; die gemeldete
`serverInfo.version` (siehe `initialize`) entspricht dem SemVer aus `Cargo.toml`.
Ein neues Release entsteht durch einen Git-Tag `vX.Y.Z` — die CI baut daraus
automatisch das gleichnamige Image.

## Aus dem Quellcode bauen & testen

```bash
cargo build --workspace
cargo test  --workspace                 # Unit-/Integrationstests, kein Netzwerk
cargo test  --workspace -- --ignored      # Live-Konformanz gegen Fedlex (Netzwerk)
```

## Architektur & Entscheidungen

- LikeC4-Architekturplan: [`likec4/`](./likec4)
- Capability-Lexikon (JOLux-Funktionsraum): [`docs/10_LEXICON_jolux.md`](./docs/10_LEXICON_jolux.md)
- Umsetzungsplan & Checkliste: [`docs/30_PLAN.md`](./docs/30_PLAN.md)
- Offene Punkte & Nutzbarkeit: [`docs/60_OPEN_ITEMS_AND_USABILITY.md`](./docs/60_OPEN_ITEMS_AND_USABILITY.md)
- Architecture Decision Records: [`docs/adr/`](./docs/adr)
- Mitwirken: [`CONTRIBUTING.md`](./CONTRIBUTING.md) · Sicherheit: [`SECURITY.md`](./SECURITY.md)

## Lizenz

[Apache-2.0](./LICENSE) © mindful.bio

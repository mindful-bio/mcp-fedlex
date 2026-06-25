# mcp-fedlex

[![Release](https://img.shields.io/badge/release-v0.2.0-green.svg)](./CHANGELOG.md)
[![MCP](https://img.shields.io/badge/MCP-2025--11--25-blue.svg)](./docs/adr/ADR-008-mcp-protocol-version-upgrade.md)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](./Cargo.toml)

🇩🇪 Deutsch · [🇬🇧 English](./README.md) · [🇫🇷 Français](./README.fr.md) · [🇮🇹 Italiano](./README.it.md)


Ein **Model-Context-Protocol-Server für Fedlex** (Schweizer Bundesrecht) — ein
Produkt von [mindful.bio](https://mindful.bio). Er gibt einem LLM **belegbaren**
Zugriff auf konsolidiertes Bundesrecht, statt es frei formulieren zu lassen:


> 📖 **Ausführliche Projektbeschreibung** (fünfsprachig: Werkzeuge, Schnellstart,
> Architektur): **[mcp-fedlex.ch](https://mcp-fedlex.ch)**.
> Live-Anwendungsplattform, die auf diesem Server aufsetzt: **[ansv.ch](https://ansv.ch)**.
> Siehe auch den Abschnitt [Ökosystem](#ökosystem) unten.


- 📌 **Provenance per Konstruktion** — jede Antwort trägt ihren `eli` und den
  `valid_as_of`-Stichtag. Der Stichtag wird **serverseitig gestempelt** und kann
  von keinem Tool-Argument verfälscht werden. Strukturell unterschieden: ein
  **Norm-Beleg** (`kind: "norm"`) vs. ein **Discovery-Hinweis** (`kind: "hint"`,
  Kandidat — *kein* Beleg), sodass ein Reasoner einen Treffer nie versehentlich
  als belegte Norm verbucht.
- 🔒 **Least-Privilege-RBAC** — 25 Werkzeuge in vier aktiven Pools, nach Rolle
  gefiltert (Reader ⊆ Navigator ⊆ Validator). Identität stammt immer aus dem
  geprüften Credential, nie aus einem LLM-Parameter.
- 🧯 **Mandantentrennung & Quota** — pro Token serverseitig durchgesetzt
  (verteiltes, fail-closed Token-Bucket über Redis); Live-Discovery wiegt im
  Quota schwerer als lokale Navigation, um den öffentlichen Fedlex-Endpunkt zu
  schützen.
- 🧾 **Audit-Log pro Aufruf** — jede `tools/call`-Zeile hält Mandant, Session,
  Tool, ELID und Stichtag fest; Roh-Argumente und Antwortinhalte werden
  fail-closed redacted (PII-Scrubber, ADR-001).
- 🦀 **Rust, kein Netz im Test** — Unit-/Integrationstests laufen offline;
  Live-Konformanz gegen Fedlex ist separat (`-- --ignored`).

> **Hinweis:** Dieses GitHub-Repository ist ein **öffentlicher Spiegel**. Die
> Quelle der Wahrheit (CI/CD, Releases) liegt auf einer selbst-gehosteten GitLab;
> Issues und PRs hier werden gesichtet, aber dort verarbeitet.

## Was er kann

25 Werkzeuge in vier aktiven Pools, RBAC-gefiltert (Reader ⊆ Navigator ⊆ Validator).
Der **Reader** sieht nur `LocalNavigation`; **Navigator** (so läuft ansV) zusätzlich
`Discovery` und `JoluxMetadata`; **Validator** zusätzlich `Validation`.

**Navigation im Erlasstext (AKN, Pool `LocalNavigation`, 11 Tools)**
`read_article` · `read_element` · `read_document` · `get_structure` · `search_text`
· `get_metadata` · `get_references` · `get_modifications` · `list_components`
· `extract_tables` · `detect_foreign_content`.

**Auffinden von Erlassen (Pool `Discovery`, 3 Tools)**
`search_law` · `resolve_sr_number` · `find_related_topic`. Treffer tragen
**Hinweis-Provenance** (`kind: "hint"`) — Kandidaten, kein Norm-Beleg.

**Metadaten & Beziehungen (JOLux, Pool `JoluxMetadata`, 10 Tools)**
`check_in_force` · `list_versions` · `resolve_consolidation_at` · `get_impacts` ·
`get_outgoing_impacts` · `get_article_history` · `get_citations` · `get_taxonomy` ·
`get_subdivisions` · `list_annexes`.

**Validierung (Pool `Validation`, 1 Tool)**
`compare_versions` (Versionsvergleich, nur Validator).

> `Discovery` und `JoluxMetadata` gehen **live** an den öffentlichen
> Fedlex-SPARQL-Endpunkt und wiegen im Quota schwerer (Cost-Gewicht 5 statt 1),
> während `LocalNavigation` aus dem Manifestations-Cache des Pods bedient wird.

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

Der Server spricht MCP über JSON-RPC (Protokoll `2025-11-25`; ein explizit
`2024-11-05` anfragender Alt-Client erhält weiterhin `2024-11-05`). Es gibt drei
HTTP-Routen:

- **`POST /mcp`** — der **Streamable-HTTP-Endpoint** der Ziel-Revision
  `2025-11-25` (empfohlen). Setzt zwei Transport-Wächter *vor* jeder Arbeit durch:
  ein fremder `Origin`-Header wird mit **403** abgewiesen (DNS-Rebinding-Schutz),
  ein gesetzter, aber nicht unterstützter `MCP-Protocol-Version`-Header mit **400**.
- **`POST /rpc`** — der Legacy-Endpoint (gleiche `McpService`-Kette ohne die
  beiden Wächter). Bleibt für handshake-lose Alt-Clients erhalten.
- **`GET /sse`** — eröffnet den SSE-Strom und nennt `/rpc` als POST-Adresse.

Methoden: `initialize` (Handshake mit Versions-Negotiation), `tools/list`
(RBAC-gefiltert), `tools/call` (Quota-gedrosselt, durch das Provenance-Gate),
`ping` (Keep-alive) sowie die Notification `notifications/initialized`
(mit **202 Accepted** ohne Body quittiert). **Auth:** Bearer-Token im
`Authorization`-Header bei **jeder** Anfrage (außer Notifications).

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

### Im Browser inspizieren (MCP Inspector)

Um die Werkzeuge interaktiv durchzuklicken, eignet sich der offizielle
[MCP Inspector](https://github.com/modelcontextprotocol/inspector). Eine fertige
[`inspector.json`](./inspector.json) liegt im Repo, sodass **ein einziger Befehl**
mit vorausgefüllter URL **und** Token verbindet — ohne manuelles Klicken in der UI:

```bash
npx -y @modelcontextprotocol/inspector --config inspector.json --server fedlex
```

Der Inspector öffnet sich im Browser bereits **verbunden**; wähle den Tab
**Tools**, z. B. `read_article` mit `eli = eli/cc/1999/404` und `eid = art_1`.

Die `inspector.json` zeigt auf `http://localhost:8090/mcp`, weil der Host-Port
überschreibbar ist — falls `8080` belegt ist, starte den Server mit einem anderen:

```bash
MCP_HOST_PORT=8090 docker compose up --build   # Port im Container bleibt 8080
```

Der Container lauscht immer auf `8080`; `MCP_HOST_PORT` mappt nur die Host-Seite um
(siehe `docker-compose.yml`). Halte den Wert in `inspector.json` damit konsistent.

> Lieber von Hand konfigurieren? Transport **Streamable HTTP**, URL
> `http://localhost:8090/mcp` und ein **aktivierter** Header
> `Authorization: Bearer <Token>`. Default-Token: `dev-secret-change-me` (deine `.env`).

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
- Review-Findings (lebendes Register): [`docs/65_REVIEW_FINDINGS.md`](./docs/65_REVIEW_FINDINGS.md)
- Architecture Decision Records: [`docs/adr/`](./docs/adr)
- Mitwirken: [`CONTRIBUTING.md`](./CONTRIBUTING.md) · Sicherheit: [`SECURITY.md`](./SECURITY.md)

## Ökosystem

`mcp-fedlex` ist der provenance-gesicherte Daten-Layer einer kleinen Produkt-Familie
von [mindful.bio](https://mindful.bio):

| Projekt | Was es ist | Link |
|---------|-----------|------|
| **mcp-fedlex** (dieses Repo) | Der MCP-Server: belegbarer, stichtagsgenauer Zugriff auf Schweizer Bundesrecht. Ausführliche, fünfsprachige Projektbeschreibung (Werkzeuge, Schnellstart, Architektur). | **[mcp-fedlex.ch](https://mcp-fedlex.ch)** |
| **ansV** | Die **Anwendungsplattform**, die diesen Server als Navigator-Client nutzt — juristische Analysen mit nachvollziehbarer Belegkette. | **[ansv.ch](https://ansv.ch)** |
| **mindful.bio** | Das Unternehmen hinter beiden Projekten. | **[mindful.bio](https://mindful.bio)** |

## Lizenz

[Apache-2.0](./LICENSE) © mindful.bio


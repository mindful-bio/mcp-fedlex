# mcp-fedlex

[![Release](https://img.shields.io/badge/release-v0.2.0-green.svg)](./CHANGELOG.md)
[![MCP](https://img.shields.io/badge/MCP-2025--11--25-blue.svg)](./docs/adr/ADR-008-mcp-protocol-version-upgrade.md)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-edition%202024-orange.svg)](./Cargo.toml)

[🇬🇧 English](./README.md) · [🇩🇪 Deutsch](./README.de.md) · [🇫🇷 Français](./README.fr.md) · 🇮🇹 Italiano

Un **server Model-Context-Protocol per Fedlex** (diritto federale svizzero) — un
prodotto di [mindful.bio](https://mindful.bio). Offre a un LLM un accesso
**documentabile** al diritto federale consolidato, invece di lasciarlo formulare
liberamente:

> 📖 **Descrizione completa del progetto** (cinque lingue: strumenti, avvio
> rapido, architettura): **[mcp-fedlex.ch](https://mcp-fedlex.ch)**.
> Piattaforma applicativa in produzione basata su questo server:
> **[ansv.ch](https://ansv.ch)**. Vedi anche la sezione
> [Ecosistema](#ecosistema) più sotto.

- 📌 **Provenienza per costruzione** — ogni risposta porta il proprio `eli` e la
  data di riferimento `valid_as_of`. La data è **marcata lato server** e non può
  essere falsificata da alcun argomento di strumento. Distinzione strutturale: una
  **prova normativa** (`kind: "norm"`) vs. un **indizio di scoperta**
  (`kind: "hint"`, candidato — *non* una prova), così che un motore di
  ragionamento non registri mai per errore un risultato come norma comprovata.
- 🔒 **RBAC a privilegio minimo** — 25 strumenti in quattro pool attivi, filtrati
  per ruolo (Reader ⊆ Navigator ⊆ Validator). L'identità proviene sempre dalla
  credenziale verificata, mai da un parametro del LLM.
- 🧯 **Isolamento dei tenant & quota** — applicati lato server per token (token
  bucket distribuito e fail-closed tramite Redis); la scoperta in tempo reale pesa
  di più nella quota rispetto alla navigazione locale, per proteggere l'endpoint
  pubblico di Fedlex.
- 🧾 **Registro di audit per chiamata** — ogni riga `tools/call` registra tenant,
  sessione, strumento, ELID e data di riferimento; gli argomenti grezzi e il
  contenuto delle risposte vengono oscurati in modalità fail-closed
  (scrubber PII, ADR-001).
- 🦀 **Rust, nessuna rete nei test** — i test unitari e di integrazione vengono
  eseguiti offline; la conformità in tempo reale contro Fedlex è separata
  (`-- --ignored`).

> **Nota:** questo repository GitHub è un **mirror pubblico**. La fonte di verità
> (CI/CD, release) risiede su un GitLab self-hosted; le issue e le PR qui vengono
> esaminate, ma elaborate lì.

## Cosa sa fare

25 strumenti in quattro pool attivi, filtrati tramite RBAC
(Reader ⊆ Navigator ⊆ Validator). Il **Reader** vede solo `LocalNavigation`; il
**Navigator** (modalità di esecuzione di ansV) aggiunge `Discovery` e
`JoluxMetadata`; il **Validator** aggiunge inoltre `Validation`.

**Navigazione nel testo dell'atto (AKN, pool `LocalNavigation`, 11 strumenti)**
`read_article` · `read_element` · `read_document` · `get_structure` · `search_text`
· `get_metadata` · `get_references` · `get_modifications` · `list_components`
· `extract_tables` · `detect_foreign_content`.

**Scoperta di atti (pool `Discovery`, 3 strumenti)**
`search_law` · `resolve_sr_number` · `find_related_topic`. I risultati portano una
**provenienza di indizio** (`kind: "hint"`) — candidati, non prove normative.

**Metadati & relazioni (JOLux, pool `JoluxMetadata`, 10 strumenti)**
`check_in_force` · `list_versions` · `resolve_consolidation_at` · `get_impacts` ·
`get_outgoing_impacts` · `get_article_history` · `get_citations` · `get_taxonomy` ·
`get_subdivisions` · `list_annexes`.

**Validazione (pool `Validation`, 1 strumento)**
`compare_versions` (confronto tra versioni, solo Validator).

> `Discovery` e `JoluxMetadata` interrogano **in tempo reale** l'endpoint SPARQL
> pubblico di Fedlex e pesano di più nella quota (costo 5 invece di 1), mentre
> `LocalNavigation` è servito dalla cache delle manifestazioni del pod.

## Operativo in locale in 2 minuti

Prerequisito: Docker con Compose. Nessuna toolchain Rust necessaria.

```bash
cp .env.example .env          # imposta il token di dev & co. (i default bastano per i test)
docker compose up --build     # avvia Reader + Redis
```

Il Reader resta poi in ascolto su `http://localhost:8080`. Verifica della salute:

```bash
curl -s http://localhost:8080/livez      # -> "ok" (liveness)
curl -s http://localhost:8080/readyz      # -> verifica Redis + Fedlex SPARQL
```

Elenca gli strumenti (token di dev dal tuo `.env`):

```bash
TOKEN=dev-secret-change-me
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | jq
```

Leggi un articolo della Costituzione federale a una data di riferimento:

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

La risposta contiene il testo della norma **e** un blocco `provenance`
(`eli`, `valid_as_of`). Il parametro opzionale `as_of` (ISO `YYYY-MM-DD`) imposta
la data di riferimento; senza di esso vale la data odierna.

## Collegarsi a un client MCP

Il server parla MCP via JSON-RPC (protocollo `2025-11-25`; un client legacy che
richiede esplicitamente `2024-11-05` riceve comunque `2024-11-05`). Esistono tre
route HTTP:

- **`POST /mcp`** — l'**endpoint Streamable HTTP** della revisione obiettivo
  `2025-11-25` (consigliato). Applica due guardie di trasporto *prima* di qualsiasi
  lavoro: un header `Origin` estraneo viene respinto con **403** (protezione dal
  DNS rebinding), un header `MCP-Protocol-Version` impostato ma non supportato con
  **400**.
- **`POST /rpc`** — l'endpoint legacy (stessa catena `McpService` senza le due
  guardie). Mantenuto per i client legacy senza handshake.
- **`GET /sse`** — apre il flusso SSE e indica `/rpc` come indirizzo POST.

Metodi: `initialize` (handshake con negoziazione della versione), `tools/list`
(filtrato tramite RBAC), `tools/call` (limitato dalla quota, attraverso il gate di
provenienza), `ping` (keep-alive), oltre alla notifica
`notifications/initialized` (confermata con **202 Accepted** senza corpo).
**Auth:** token Bearer nell'header `Authorization` a **ogni** richiesta
(eccetto le notifiche).

Esempio `initialize`:

```bash
curl -s -X POST http://localhost:8080/rpc \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":0,"method":"initialize"}' | jq
# -> { "protocolVersion":"2025-11-25",
#      "serverInfo":{"name":"mcp-fedlex-reader", ...},
#      "capabilities":{"tools":{}} }
```

> Senza `protocolVersion` nella richiesta, il server negozia la revisione
> predefinita `2025-11-25`. Un client che invia esplicitamente
> `"protocolVersion":"2024-11-05"` riceve comunque `2024-11-05` (compatibilità
> all'indietro per i client legacy).

Per i client con configurazione JSON (p. es. Claude Desktop tramite un connettore
ponte SSE/HTTP), bastano l'URL di base `http://localhost:8080` e il token Bearer.

### Ispezionare nel browser (MCP Inspector)

Per esplorare gli strumenti in modo interattivo, usa l'
[MCP Inspector](https://github.com/modelcontextprotocol/inspector) ufficiale. Nel
repo è incluso un file [`inspector.json`](./inspector.json) già pronto, così
**un solo comando** si connette con URL **e** token precompilati — senza armeggiare
manualmente con l'interfaccia:

```bash
npx -y @modelcontextprotocol/inspector --config inspector.json --server fedlex
```

L'Inspector si apre nel browser già **connesso**; scegli la scheda **Tools**,
ad es. `read_article` con `eli = eli/cc/1999/404` e `eid = art_1`.

`inspector.json` punta a `http://localhost:8090/mcp` perché la porta host è
sovrascrivibile — se `8080` è già occupata, avvia il server su un'altra:

```bash
MCP_HOST_PORT=8090 docker compose up --build   # la porta nel container resta 8080
```

Il container ascolta sempre su `8080`; `MCP_HOST_PORT` rimappa solo il lato host
(vedi `docker-compose.yml`). Mantieni coerente il valore in `inspector.json`.

> Preferisci configurare a mano? Trasporto **Streamable HTTP**, URL
> `http://localhost:8090/mcp` e un header **attivato**
> `Authorization: Bearer <token>`. Token predefinito: `dev-secret-change-me` (il tuo `.env`).

## Configurazione

L'intera configurazione passa per variabili d'ambiente. Riferimento completo con
valori predefiniti e campi obbligatori:
**[`docs/70_CONFIG.md`](./docs/70_CONFIG.md)**. Il modello di ruoli e token
(token di dev vs. JWT/JWKS) è in
**[`docs/90_AUTH_AND_ROLES.md`](./docs/90_AUTH_AND_ROLES.md)**.

> La configurazione Compose è pensata per lo **sviluppo** (Redis in chiaro, token
> di dev statico). Esercizio in produzione su Kubernetes (JWT/JWKS, Redis mTLS,
> SealedSecrets): **[`docs/80_DEPLOY.md`](./docs/80_DEPLOY.md)**.

## Ottenere un'immagine versionata

Per l'uso da parte di terzi esistono **immagini SemVer citabili** (immutabili,
legate a un tag Git) in aggiunta ai tag mobili del continuous deploy interno:

| Tag | Scopo | Stabilità |
|-----|-------|-----------|
| `:v0.2.0` | release citabile (legata al tag Git `v0.2.0`, MCP `2025-11-25`) | immutabile — **consigliata per terzi** |
| `:v0.1.0` | release più vecchia (legata al tag Git `v0.1.0`, MCP `2024-11-05`) | immutabile |
| `:latest` | ultimo stato di `main` | mobile |
| `:<short-sha>` | commit esatto | immutabile, interno |

```bash
docker pull registry.mindful-server.com/mindful-bio/mcp-fedlex:v0.2.0
```

Le release sono documentate in [`CHANGELOG.md`](./CHANGELOG.md); la
`serverInfo.version` riportata (vedi `initialize`) corrisponde al SemVer di
`Cargo.toml`. Una nuova release nasce da un tag Git `vX.Y.Z` — la CI costruisce
automaticamente l'immagine con lo stesso nome.

## Compilare & testare dal sorgente

```bash
cargo build --workspace
cargo test  --workspace                 # test unitari/di integrazione, senza rete
cargo test  --workspace -- --ignored      # conformità in tempo reale contro Fedlex (rete)
```

## Architettura & decisioni

- Piano di architettura LikeC4: [`likec4/`](./likec4)
- Lessico delle capacità (spazio funzionale JOLux): [`docs/10_LEXICON_jolux.md`](./docs/10_LEXICON_jolux.md)
- Piano di realizzazione & checklist: [`docs/30_PLAN.md`](./docs/30_PLAN.md)
- Punti aperti & usabilità: [`docs/60_OPEN_ITEMS_AND_USABILITY.md`](./docs/60_OPEN_ITEMS_AND_USABILITY.md)
- Esiti delle revisioni (registro vivo): [`docs/65_REVIEW_FINDINGS.md`](./docs/65_REVIEW_FINDINGS.md)
- Architecture Decision Records: [`docs/adr/`](./docs/adr)
- Contribuire: [`CONTRIBUTING.md`](./CONTRIBUTING.md) · Sicurezza: [`SECURITY.md`](./SECURITY.md)

## Ecosistema

`mcp-fedlex` è il livello dati a provenienza garantita di una piccola famiglia di
prodotti di [mindful.bio](https://mindful.bio):

| Progetto | Di cosa si tratta | Link |
|----------|-------------------|------|
| **mcp-fedlex** (questo repo) | Il server MCP: accesso documentabile e con data di riferimento al diritto federale svizzero. Descrizione completa del progetto in cinque lingue (strumenti, avvio rapido, architettura). | **[mcp-fedlex.ch](https://mcp-fedlex.ch)** |
| **ansV** | La **piattaforma applicativa** che usa questo server come client Navigator — analisi giuridiche con una catena di prove tracciabile. | **[ansv.ch](https://ansv.ch)** |
| **mindful.bio** | L'azienda dietro entrambi i progetti. | **[mindful.bio](https://mindful.bio)** |

## Licenza

[Apache-2.0](./LICENSE) © mindful.bio

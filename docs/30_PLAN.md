# 30 — Umsetzungsplan mcp-fedlex

Lebende Checkliste für die schrittweise Implementierung. Jeder Meilenstein liefert einen
beweisbar grünen Test (`cargo test`), bevor der nächste beginnt. Die Akzeptanzkriterien der
ADRs sind die fachliche Messlatte, dieser Plan ist die Reihenfolge.

**Leitprinzip.** Walking Skeleton zuerst, dann Schicht für Schicht. Kein Schritt gilt als
erledigt ohne Test, der die Eigenschaft beweist. Wir bauen entlang der LikeC4-Container, von
innen (Kern-Typen) nach aussen (Transport, Föderation).

## Ziel-Workspace (Cargo)

Die Crates spiegeln die LikeC4-Container. So bleibt Architektur und Code deckungsgleich.

```
mcp-fedlex/
  Cargo.toml                 # Workspace-Root
  rust-toolchain.toml        # gepinnte Toolchain
  crates/
    fedlex-core/             # Kern-Typen: ELI, Provenance, Bi-Temporalität, Sensitive<T>
    fedlex-store/            # Redis-L2 + Oxigraph, Tenant-Repository (Data-Access-Layer)
    fedlex-telemetry/        # tracing-Layer + PII-Scrubber (ADR-001)
    mcp-reader/              # Reader-Binary: SSE, Auth, Quota, Registry, Tools, Engine, LOD
    mcp-ingest/              # Writer-Binary: Event-Consumer, Parser, Indexer, Outbox, DLQ
  tests/                     # crate-übergreifende Integrationstests (Testcontainers)
```

## Test-Strategie

- **Unit-Tests** neben dem Code (`#[cfg(test)] mod tests`) für reine Logik (Parser, Resolver, Token-Bucket-Mathematik, Provenance-Hülle).
- **Integrationstests** in `crates/*/tests/` und Workspace-`tests/` für Store-Zugriff und Transport, mit Testcontainers (Redis, Oxigraph) statt Mocks, wo echtes Verhalten zählt.
- **Property-Tests** (`proptest`) für die Sicherheits-Invarianten (PII-Scrubbing, Key-Injection, Tenant-Isolation).
- **Negativtests** als Beweis je ADR, dass die verbotene Sache hart fehlschlägt.
- **CI-Gate.** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` müssen grün sein.

---

## M0 — Walking Skeleton & CI

Ziel ist ein kompilierender Workspace mit grünem Test und CI, bevor Fachlogik entsteht.

- [x] Cargo-Workspace anlegen (`Cargo.toml` Root + leere Crates aus der Zielstruktur).
- [x] `rust-toolchain.toml` pinnen, `.gitignore` um `/target` ergänzen (bereits vorhanden prüfen).
- [x] `fedlex-core` mit einem trivialen Smoke-Test (`assert` auf eine Konstante).
- [x] CI-Workflow (`.github/workflows/ci.yml`) mit fmt + clippy + test.
- [x] **Test-Nachweis.** `cargo test` grün (alle Crates), `cargo fmt --check` und `cargo clippy -D warnings` grün.

## M1 — Kern-Typen (`fedlex-core`)

Die Typen, die alle Schichten teilen. Hier sitzt die Provenance-Pflicht aus ADR-004.

- [x] `Eli`-Typ (Parsing/Display, validiert) und `Ecli` für Rechtsprechung.
- [x] Bi-temporale Typen `ValidAsOf` und `TransactionTime`.
- [x] `Provenance { eli, valid_as_of, transaction_time }` und generischer `Response<T>` (ADR-004).
- [x] `Sensitive<T>`-Newtype, dessen `Debug`/`Display` redacten (ADR-001, Basis für M9).
- [x] **Test-Nachweis.** `Response<T>` ist nur über `new(data, provenance)` konstruierbar (private Felder, Gate strukturell erzwungen); `Sensitive<T>` maskiert im Debug-Output, auch eingebettet in eine Struktur. 23 Tests grün.

## M2 — Store-Layer & Tenant-Isolation (`fedlex-store`)

Der einzige Weg zu Redis und Oxigraph, mit der Isolation aus ADR-001 als Invariante.

- [x] Redis-Client (L2), Key-Schema `tenant_id:session_id:key` zentral injiziert.
- [x] `TenantRepository` als einzige Schreib-/Lese-Schnittstelle zum Scratchpad.
- [x] Key-Injection-Schutz (Separatoren `:` und Glob-Wildcards abgelehnt).
- [x] Oxigraph-Anbindung (SPARQL), append-only bi-temporale Schreibpfade. Nachgeholt als Lücke B. Eingebetteter In-Process-Oxigraph mit echtem SPARQL, Gueltigkeits- und Transaktionszeit getrennt, Korrektur per hoeherer Transaktionszeit ohne Verlust der Historie.
- [x] **Test-Nachweis.** Negativtest (Testcontainers Redis), dass ein Zugriff mit fremder/erfundener `tenant_id` und mit Ausbruchs-Keys hart fehlschlägt; Property-Test gegen Key-Injection.

## M3 — Transport, Auth & verteiltes Quota (`mcp-reader`)

Die Eingangstür. Hier sitzt das verteilte Token-Bucket aus ADR-002.

- [x] axum-SSE-Endpunkt (`sseHandler`), JSON-RPC-MCP-Nachrichten. Nachgeholt als Lücke A. Der Transport verdrahtet Auth, Quota, Temporal-Stempel und Registry-Dispatch Ende zu Ende.
- [x] `authRbac` gegen IdP/Vault (mockbar im Test), Rolle aus validiertem Claim.
- [x] `rateLimiter` als Redis-Lua-Token-Bucket, atomar (ADR-002), `tenant_id` nur aus Claim.
- [x] Fail-closed-Fallback bei Redis-Ausfall (konservatives Pod-Limit).
- [x] **Test-Nachweis.** Nebenläufigkeits-Test, dass das Gesamtlimit über mehrere simulierte Pods eingehalten wird; Test, dass Quota nicht aus LLM-Parametern manipulierbar ist.

## M4 — MCP-Registry, Tools & Provenance-Gate (`mcp-reader`)

Die MCP-Oberfläche mit dem verpflichtenden Antwort-Gate aus ADR-004.

- [x] `trait McpTool` (`execute()` + `schema()`), RBAC-Tool-Pools (Least-Privilege).
- [x] `temporalResolver` stempelt Anfragen mit `valid_as_of`/`transaction_time`.
- [x] `provenanceEnvelope` als Gate, das jede Antwort durch `Response<T>` zwingt (ADR-004).
- [x] `gracefulFailure`-Middleware (Fehler zu `{ error, hint }`).
- [x] **Test-Nachweis.** `execute` liefert strukturell `Response<Value>`, eine Antwort ohne Provenance ist nicht konstruierbar (privates Feld); Dispatch-Test zeigt Provenance im Wire-Format; `tools/list` rollenabhängig gefiltert; Graceful-Failure statt Crash.

## M5 — XML/AKN-Engine & Sandbox (`mcp-reader`)

Read-Path-Heavy-Lifting mit DoS-Schutz.

- [x] AKN/Jolux-Parsing zu DOM, `l1Cache` (moka, Single-Flight via `get_with`).
- [x] Makro-Tools (Version-Diff zu destilliertem Markdown), Paginierungs-Guard.
- [x] `execSandbox` (spawn_blocking, harte Deadline ~2s, Linearzeit-Regex mit size_limit).
- [x] **Test-Nachweis.** Test, dass ein bewusst teurer XPath/Regex-Query nach Deadline abbricht und Graceful-Failure liefert; L1-Single-Flight-Test (ein Parse pro Key).

## M6 — LOD-Gateway & Circuit Breaker (`mcp-reader`)

Föderierte Auflösung über ELI/ECLI.

- [x] `eliResolver` (lokal-vs-extern-Entscheidung), SPARQL-Walks im Oxigraph.
- [x] `circuitBreaker` (open/half-open/closed), `httpConnector`-Pool nur über Breaker.
- [x] **Test-Nachweis.** Test, dass ein langsamer/defekter externer Endpunkt den Breaker öffnet und kein Task-Aufstauen erzeugt; Test, dass lokale Referenzen ohne externen Call aufgelöst werden.

## M7 — Semantic-Client (`mcp-reader`)

Dünner, optionaler Client zu semantic-fedlex.

- [x] `semanticClient.search(query, as_of, top_k)`, Top-K-Mapping mit Provenance je Treffer (ADR-004).
- [x] Graceful Degradation bei Ausfall (rein strukturelle Navigation).
- [x] **Test-Nachweis.** Test, dass bei nicht erreichbarem Dienst der Reader weiter antwortet; Test, dass jeder Treffer seine eigene Provenance trägt.

## M8 — Writer, Embedding-Outbox & DLQ (`mcp-ingest`)

Der Schreibpfad mit der Resilienz aus ADR-003.

- [x] `eventConsumer` (Kafka/NATS), Dedup, DLQ nach N Retries mit Kontext (ADR-003).
- [x] `ingestParser` (gigabytegrosse AKN/Jolux nativ), `indexWriter` idempotent.
- [x] Oxigraph-Anbindung (SPARQL), append-only bi-temporale Schreibpfade (aus M2 verschoben). Der `OxigraphCorpusSink` verdrahtet den Schreibpfad an den eingebetteten Korpus der Leseseite. Die Fassungs-Kennung wird zum Stichtag, der Schreibmoment zur Transaktionszeit. Die Senke ist fehlbar, ein Store-Ausfall laeuft ueber Retry und DLQ statt still verloren zu gehen.
- [x] `embeddingOutbox` transaktional, idempotenter Zusteller mit Backoff, Vollständigkeits-Marker.
- [x] Cache-Invalidierungs-Events nach jedem Write.
- [x] **Test-Nachweis.** Test, dass ein Poison-Release in der DLQ landet und valide Releases nicht blockiert; Test, dass ein simulierter semantic-fedlex-Ausfall die Outbox füllt und nach Recovery nachzieht (kein stiller Drift).

## M9 — Observability & PII-Scrubber (`fedlex-telemetry`)

Lückenloses Tracing mit Compliance-Gate.

- [x] `#[tracing::instrument]`-Abdeckung, OTLP-Export.
- [x] `piiScrubber` allowlist-basiert, Redaction am Entstehungspunkt (ADR-001).
- [x] **Test-Nachweis.** Snapshot-/Property-Test, dass kein Roh-Tool-Argument und keine Roh-Response unmaskiert in einen exportierten Span gelangt.

## M10 — Betriebshärtung (Day-2)

- [ ] Service-to-Service-mTLS + Default-Deny-NetworkPolicy (ADR-005). Infra (Nix/K8s), nicht per cargo-test beweisbar. Die Default-Deny-NetworkPolicy ist als Manifest gesetzt (siehe `k3-infra`), mTLS bleibt offen.
- [x] Liveness/Readiness/Startup-Health-Endpunkte im Reader (Backlog B-2). Getrennte Signale, Liveness ohne Abhaengigkeit, Readiness ueber benannte Bereitschaftspruefungen, Startup gegen zu fruehen Neustart. 5 Tests gruen per tower oneshot.
- [x] Server-Komposition und Binaer. `app()` verschmilzt Transport und Health zu einer App, `serve()` und `main()` haengen sie an einen echten Socket. 3 Tests gruen (oneshot auf die App, Startup-Flip, echte TCP-Bind-Anfrage gegen `/livez`). Damit zeigen die K8s-Proben auf reale Pfade.
- [x] K8s-Manifeste, Verdrahtung der Probes an `/livez`, `/readyz`, `/startupz` (Backlog B-2). In `k3-infra/manifests/workloads/mcp-fedlex` (Reader-Deployment mit drei Proben, Redis-Quota-Backend, Service, Argo-Application). Statisch gerendert und client-validiert per `kubectl`, kein cargo-test-Beweis.
- [ ] Oxigraph-Backup/Restore (Backlog B-1). Infra, nicht per cargo-test beweisbar.
- [x] Cache-Warmup gegen Stampede (Backlog B-1). Single-Flight ueber moka `try_get_with`, fehlbarer Lader ohne Cache-Vergiftung, proaktives Batch-Vorwaermen mit Bericht. 5 Tests gruen.
- [ ] AKN/Jolux-Schema-Versions-Handling (Backlog B-2).
- [ ] **Test-Nachweis.** Negativtest, dass ein nicht-authentifizierter In-Cluster-Client Redis/Oxigraph/semantic-fedlex nicht erreicht; Restore-Test, der den Graph aus Backup wiederherstellt. Ehrliche Einordnung. Nur der Health-Endpunkt-Teil hat einen cargo-test-Beweis, der Rest ist Infra.

## M11 — Discovery-Tools & Hinweis-Provenance (`mcp-reader`, `fedlex-core`)

Schliesst die Discovery-Lücke der MCP-Oberfläche (siehe `40_FINDINGS.md`, ADR-006). Die
jolux-Primitive existieren und sind live-getestet; hier entsteht nur die dünne Tool-Projektion
plus die strukturelle Hinweis/Beleg-Trennung.

- [ ] `Provenance::Hint` (bzw. `kind`-Feld) in `fedlex-core`, im Wire-Format als `"hint"` vs.
      `"norm"` unterscheidbar; `QueryStamp` kann eine Hinweis-Provenance je Treffer-ELI bilden.
- [ ] `ToolPool::Discovery` in `mcp-reader`, RBAC. `Reader` ohne, `Navigator`/`Validator` mit
      Discovery (ADR-006).
- [ ] Drei `McpTool`-Wrapper über die Primitive. `search_law` (Titel/Stichwort),
      `resolve_sr_number` (SR-Nummer), `find_related_topic` (ELI → thematisch verwandt). Jeder
      Treffer trägt seine eigene Hinweis-Provenance (ADR-004 „Listen/Aggregate").
- [ ] `register_discovery_tools` analog `register_navigation_tools`, verdrahtet im Reader-Binary
      gegen `HttpSparqlClient::fedlex()`.
- [ ] Pool-abhängiges Quota-Cost-Gewicht. `Discovery` schwerer als `LocalNavigation`,
      claim-gebunden (ADR-002/ADR-006 F-5).
- [ ] **Test-Nachweis.** `tools/list` listet Discovery rollenabhängig (Reader ohne); Dispatch-Test
      zeigt Hinweis-Provenance im Wire-Format (`kind: "hint"`); Test der Discovery→Beleg-Kette
      (Treffer-ELI ist anschliessend von `get_metadata`/`read_article` auflösbar); Quota-Test, dass
      ein Discovery-Call mehr Tokens bucht als ein Navigationscall.

---

## Fortschritt


| Meilenstein | Status | Beweis |
| --- | --- | --- |
| M0 Skeleton & CI | erledigt | `cargo fmt --check` + `clippy -D warnings` + `cargo test` grün |
| M1 Kern-Typen | erledigt | 23 Tests grün (Eli/Ecli, Temporal, Provenance, Response-Gate, Sensitive) |
| M2 Store & Tenant | erledigt | 13 Unit/Property-Tests grün + 2 Docker-gated Redis-Integrationstests (Cross-Tenant/Session/Injection) |
| M3 Transport & Quota | erledigt | Auth/RBAC + Fail-closed-Quota, 8 Unit-Tests grün, Redis-Integration (globales Limit über 4 Pods, Claim-gebundener Bucket) |
| M4 Registry & Provenance | erledigt | 15 Unit-Tests grün (Provenance-Gate im Wire-Format, RBAC-gefiltertes tools/list, Graceful-Failure, Temporal-Stamp) |
| M5 XML-Engine & Sandbox | erledigt | 25 lib-Tests grün, Deadline-Abbruch + L1-Single-Flight bewiesen |
| M6 LOD-Gateway | erledigt | 32 lib-Tests grün, Breaker-Open ohne Task-Aufstauen + lokale Auflösung ohne Call bewiesen |
| M7 Semantic-Client | erledigt | 35 lib-Tests grün, Graceful Degradation bei Ausfall + Provenance je Treffer bewiesen |
| M8 Writer & Outbox/DLQ | erledigt | 11 Tests grün, Poison zu DLQ ohne Blockade + Outbox-Recovery ohne stillen Drift bewiesen |
| M9 Observability & PII | erledigt | 7 Tests grün inkl. 2 Property-Tests, kein Roh-Argument/keine Roh-Response unmaskiert im Span |
| Lücke A Transport | erledigt | 44 lib-Tests grün, 9 neue Transport-Tests. JSON-RPC über Auth/Quota/Temporal/Registry, HTTP-Ebene per tower oneshot (initialize, tools/list RBAC-gefiltert, tools/call mit Provenance im Wire-Format, Quota-Drosselung als lenkende Antwort, Auth-Ablehnung) |
| Lücke B Oxigraph | erledigt | 6 Tests grün (Feature oxigraph-store), eingebetteter In-Process-Oxigraph. Bi-temporale Punkt-in-Zeit-Aufloesung, Korrektur per Transaktionszeit ohne Historienverlust, Append-only-Zaehler, SPARQL-Injection-Schutz |
| M10 Härtung | teilweise | Health-Endpunkte (5 Tests), Server-Komposition mit echtem Socket (3 Tests) und Cache-Warmup gegen Stampede (5 Tests) erledigt. K8s-Manifeste in `k3-infra` (Reader-Deployment mit drei Proben, Redis, Default-Deny-NetworkPolicy, Argo-Application), client-validiert. mTLS, durabler Korpus mit Backup/Restore und Schema-Versionierung bleiben Infra ohne cargo-test-Beweis |
| Schreib- trifft Lesepfad | erledigt | CorpusSink fehlbar gemacht (Store-Fehler ueber Retry/DLQ statt stiller Verlust). OxigraphCorpusSink-Adapter (Feature oxigraph-store) schreibt in denselben bi-temporalen Korpus, aus dem der Reader punkt-in-zeit aufloest. 1 neuer Writer-Fehlerpfad-Test, 4 Adapter-Tests gruen |
| M11 Discovery & Hinweis-Provenance | offen | Plan festgehalten (ADR-006, `40_FINDINGS.md`). Schliesst die Discovery-Lücke der MCP-Oberfläche (search_law/resolve_sr_number/find_related_topic + Hinweis-Provenance + Discovery-Pool). Noch kein Code |

Status je Zeile wird auf `erledigt` gesetzt, sobald der zugehörige Test-Nachweis grün ist.


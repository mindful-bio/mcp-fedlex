# Architecture Decision Records — mcp-fedlex

Verzeichnis der verbindlichen Architekturentscheidungen für `mcp-fedlex`. Jede ADR hält
Kontext, Entscheidung und Akzeptanzkriterien für das spätere Coding fest.

## Angenommen

| ADR | Titel | Betrifft |
| --- | --- | --- |
| [ADR-001](ADR-001-pii-scrubbing-and-tenant-isolation.md) | PII-Scrubbing & strikte Tenant-Isolation | Observability, Workspace, L1-Cache |
| [ADR-002](ADR-002-distributed-quota.md) | Verteiltes Quota & Rate-Limiting | Reader / Transport, Redis |
| [ADR-003](ADR-003-index-consistency.md) | Index-Konsistenz (Embedding-Outbox) & Ingestion-Resilienz (DLQ) | Writer / Ingestion |
| [ADR-004](ADR-004-response-provenance.md) | Response-Provenance-Envelope | Reader / Navigator, syllogismus-fedlex |
| [ADR-005](ADR-005-service-to-service-auth.md) | Service-to-Service-Authentifizierung (mTLS / Zero-Trust intern) | Reader & Writer, semantic-fedlex |
| [ADR-006](ADR-006-discovery-tools-and-hint-provenance.md) | Discovery-Tools & Hinweis-Provenance | Reader / Registry, RBAC, Quota, ansV |
| [ADR-007](ADR-007-jolux-metadata-tools-pool-and-quota.md) | JOLux-Metadaten-Tools: RBAC-Pool & Quota-Gewicht | Reader / Registry, RBAC, Quota, ansV |

## Vorgeschlagen (Plan, noch nicht umgesetzt)

| ADR | Titel | Betrifft |
| --- | --- | --- |
| [ADR-008](ADR-008-mcp-protocol-version-upgrade.md) | MCP-Protokollversion — Upgrade & Versions-Negotiation | Reader / Transport, alle MCP-Clients |

## Backlog (Härtung, noch nicht als ADR ausgearbeitet)


Diese Punkte sind erkannt und bewusst zurückgestellt. Sie betreffen Betriebshärtung, nicht
die fachliche Architektur, und werden vor dem produktiven Betrieb (Day-2) ausgearbeitet.

- **B-1 Durability & Disaster-Recovery.** ✅ **geschlossen (2026-06-21).** Backup/Restore des
  JOLux-Graphen ist als cargo-test-beweisbare Eigenschaft umgesetzt
  (`OxigraphCorpus::dump_to_string`/`restore_from_str`, append-only-treuer, deterministischer
  Roundtrip, 8 Unit-Tests). Der Cache-Warmup/Stampede-Schutz war bereits via moka-Single-Flight
  (`try_get_with`) erledigt.
- **B-2 Schema-Evolution & K8s-Health-Probes.** ✅ **geschlossen (2026-06-21).**
  Schema-Versions-Handling umgesetzt: eine `SCHEMA_VERSION`-Konstante wird in jeden Backup-Kopf
  geschrieben; `restore_from_str` prüft sie und bricht bei Abweichung hart mit
  `GraphError::SchemaMismatch` ab (statt still inkompatible Daten zu laden), Migrationsnotiz am
  Restore-Pfad. Die Liveness-/Readiness-/Startup-Probes pro Workload sind im Reader implementiert
  und in den k3-infra-Manifesten verdrahtet.


## Format

ADRs folgen einem leichten Nygard-Stil mit Status, Datum, Kontext, Entscheidung,
Akzeptanzkriterien, Alternativen und Konsequenzen. Der Status bleibt `Accepted (Plan)`,
bis die Akzeptanzkriterien im Code erfüllt sind.

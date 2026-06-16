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



## Backlog (Härtung, noch nicht als ADR ausgearbeitet)

Diese Punkte sind erkannt und bewusst zurückgestellt. Sie betreffen Betriebshärtung, nicht
die fachliche Architektur, und werden vor dem produktiven Betrieb (Day-2) ausgearbeitet.

- **B-1 Durability & Disaster-Recovery.** Der JOLux-Graph in Oxigraph wird nur vom Writer
  materialisiert und hat keinen definierten Backup-/Restore-Pfad. Zudem droht beim
  Cold-Start ein Thundering-Herd auf den L2-Cache. Festzulegen sind Backup-Strategie,
  Restore-Test und Cache-Warmup/Stampede-Schutz.
- **B-2 Schema-Evolution & K8s-Health-Probes.** Der `ingestParser` behandelt heute keine
  AKN-/JOLux-Schema-Versionen explizit; ein Schema-Wechsel der Quelle könnte still brechen
  (mit der DLQ aus ADR-003 landet ein solches Release immerhin sichtbar in der
  Dead-Letter-Queue statt die Pipeline zu blockieren). Ergänzend fehlen Liveness-/
  Readiness-/Startup-Probes trotz Day-2-Anspruch. Festzulegen sind Schema-Versions-Handling
  (Migrationsstrategie) und das Probe-Set pro Workload.

## Format

ADRs folgen einem leichten Nygard-Stil mit Status, Datum, Kontext, Entscheidung,
Akzeptanzkriterien, Alternativen und Konsequenzen. Der Status bleibt `Accepted (Plan)`,
bis die Akzeptanzkriterien im Code erfüllt sind.

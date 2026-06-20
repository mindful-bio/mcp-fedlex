# Changelog

Alle nennenswerten Änderungen an diesem Projekt werden hier festgehalten.

Das Format orientiert sich an [Keep a Changelog](https://keepachangelog.com/de/1.1.0/),
und das Projekt folgt [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

## [0.1.0] - 2026-06-20

Erste getaggte Version. Der `mcp-reader` ist produktiv ausgerollt und gegen
Fedlex live-konform getestet.

### Added

- **MCP-Reader-Server** (Protokoll `2024-11-05`): `initialize`, `tools/list`,
  `tools/call` über JSON-RPC (`POST /rpc`) und SSE (`GET /sse`).
- **22 Werkzeuge** in drei Pools, RBAC-gefiltert (Reader ⊆ Navigator ⊆ Validator):
  - AKN-Navigation: `read_article`, `read_element`, `get_structure`, `search_text`,
    `get_metadata`, `read_document`, `get_references`, `get_modifications`.
  - Discovery: `search_law`, `resolve_sr_number`, `find_related_topic`.
  - JOLux-Metadaten/-Beziehungen: `check_in_force`, `list_versions`,
    `resolve_consolidation_at`, `get_impacts`, `get_outgoing_impacts`,
    `get_article_history`, `get_citations`, `get_taxonomy`, `get_subdivisions`,
    `list_annexes`.
  - Validierung: `compare_versions`.
- **Provenance-Gate** (ADR-004): jede Antwort trägt `eli` + `valid_as_of`; der
  Stichtag wird serverseitig gestempelt und ist nicht über Tool-Argumente
  verfälschbar.
- **Auth fail-closed** (ADR-002): JWT (HS256/RS256/JWKS mit Rotation) sowie
  `MCP_DEV_TOKEN` für die Entwicklung; Identität nie aus LLM-Parametern.
- **Verteilte Quota** (ADR-002): Token-Bucket in Redis, pool-gewichtet, fail-closed
  bei Redis-Ausfall.
- **Service-zu-Service-mTLS** Reader ↔ Quota-Redis (ADR-005), hinter Feature
  `redis-tls`; Klartext bei vorhandenem TLS-Material wird hart abgelehnt.
- **PII-Scrubbing & Tenant-Isolation** im Audit-Log (ADR-001, Allowlist).
- **Vollständigkeits-Matrix** der Lexikon-Projektion als Offline-Test
  (`tests/lexicon_projection.rs`, G-4-Schutz).
- **Datenschicht**: jolux- (29) + akn- (12) + bridge- (3) Live-Konformanztests,
  wöchentlich in CI.
- **Onboarding/Betrieb**: `docker-compose.yml`, `.env.example`, Quickstart-README,
  `docs/70_CONFIG.md`, `scripts/smoke.sh`, `docs/80_DEPLOY.md` (k8s + Runbook),
  `docs/90_AUTH_AND_ROLES.md`, `CONTRIBUTING.md`, `SECURITY.md`.
- **Distroless-Image** (nonroot, UID 65532), per Kaniko gebaut.

[Unreleased]: https://git.mindful-server.com/mindful-bio/mcp-fedlex/-/compare/v0.1.0...main
[0.1.0]: https://git.mindful-server.com/mindful-bio/mcp-fedlex/-/tags/v0.1.0

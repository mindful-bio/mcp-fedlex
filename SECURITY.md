# Sicherheitsrichtlinie

## Verwundbarkeiten melden

Bitte melde sicherheitsrelevante Funde **vertraulich** und **nicht** als öffentliches
Issue oder Merge Request.

- **E-Mail:** security@mindful.bio
- Bitte beschreibe Auswirkung, betroffene Komponente/Version und — wenn möglich — eine
  Reproduktion (Schritte, Anfrage/Antwort, Logs ohne echte Geheimnisse).
- Wir bestätigen den Eingang in der Regel innerhalb von **3 Werktagen** und halten dich
  über den Fortschritt auf dem Laufenden.

Verantwortungsvolle Offenlegung: Bitte gib uns angemessene Zeit zur Behebung, bevor
Details öffentlich werden.

## Geltungsbereich

Dieses Repo enthält den `mcp-reader` (MCP-Server) und seine unterstützenden Crates.
Besonders relevant:

- **Authentifizierung & RBAC** (`crates/mcp-reader/src/auth.rs`, `tool.rs`) — Umgehung von
  Rollen/Mandanten, Token-Validierung, JWKS-Handling.
- **Quota/Rate-Limiting** (`crates/mcp-reader/src/quota.rs`, `fedlex-store`) — Umgehung des
  Token-Buckets, fail-open-Verhalten.
- **PII/Provenance** — Leck von Roh-Argumenten oder Antwortinhalten ins Log; falsche
  oder fehlende Provenance.
- **Service-to-Service** — Redis-mTLS (ADR-005), NetworkPolicy.

## Sicherheitsmodell (Kurzfassung)

- **fail-closed** überall: ohne gültiges Credential bzw. bei Quota-Redis-Ausfall wird
  verweigert, nicht geöffnet.
- **Identität nie aus LLM-Parametern** (ADR-002): `tenant`/`session`/`role` stammen nur
  aus geprüften Claims. Details: [`docs/90_AUTH_AND_ROLES.md`](./docs/90_AUTH_AND_ROLES.md).
- **PII-Scrubbing** im Audit-Log (ADR-001): Allowlist statt Blocklist.
- **Distroless-Image**, Least-Privilege-Pools, Default-Deny-NetworkPolicy.

## Geheimnisse

- Niemals echte Tokens/Schlüssel committen. `.env` ist `.gitignore`d; nur
  `.env.example` mit Platzhaltern gehört ins Repo.
- Produktive Secrets liegen als **SealedSecrets** im Infra-Repo (entschlüsselbar nur durch
  den Cluster-Controller). Rotation: [`docs/80_DEPLOY.md`](./docs/80_DEPLOY.md §2).
- Solltest du versehentlich ein Geheimnis veröffentlicht haben: sofort rotieren und uns
  unter security@mindful.bio informieren.

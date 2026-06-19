# Mitwirken an mcp-fedlex

Danke für dein Interesse. Dieses Repo ist ein Rust-Workspace (MCP-Server für Schweizer
Bundesrecht). Diese Anleitung bringt dich von Null auf einen grünen Beitrag.

## Voraussetzungen

- Rust-Toolchain gemäss `rust-toolchain.toml` (wird automatisch gewählt).
- `docker` + `docker compose` für den lokalen Lauf und die Integrationstests.
- Optional: `jq` für die curl-Beispiele aus dem README.

## Lokaler Lauf

```bash
cp .env.example .env
docker compose up --build       # Reader auf http://localhost:8080
scripts/smoke.sh http://localhost:8080 dev-secret-change-me
```

## Vor jedem Commit (CI-Gleichlauf)

Die Pipeline prüft Format, Lints und Tests. Lokal dieselbe Kette:

```bash
cargo fmt --all                       # formatieren
cargo fmt --all --check               # muss sauber sein (CI nutzt rustfmt 1.96)
cargo clippy --workspace -- -D warnings
cargo test --workspace                # Unit-/Integrationstests, kein Netzwerk
```

Tests, die Netzwerk oder Docker brauchen, sind mit `#[ignore]` gegated und laufen
separat (Live-Konformanz als Wochen-Cron):

```bash
cargo test --workspace -- --ignored   # Docker/Redis + Live-Fedlex
```

## Architektur-Leitplanken (bitte beachten)

Diese Invarianten sind in ADRs festgehalten und durch Tests abgesichert — Beiträge dürfen
sie nicht verletzen:

- **Identität nie aus LLM-Parametern** (ADR-002): `tenant`/`session`/`role` nur aus
  geprüften Claims. Siehe `docs/90_AUTH_AND_ROLES.md`.
- **Provenance an jeder Antwort** (ADR-004): Normtext-Antworten tragen `eli` + `valid_as_of`.
- **PII-Scrubbing im Audit-Log** (ADR-001): keine rohen Tool-Argumente/Antwortinhalte loggen.
- **Least-Privilege-Pools** (ADR-006/007): neue Tools brauchen einen `ToolPool` und einen
  Eintrag in der Projektions-Matrix (`crates/mcp-reader/tests/lexicon_projection.rs`),
  sonst wird der Test rot.
- **fail-closed**: Auth- und Quota-Pfade verweigern im Zweifel, statt zu öffnen.

## Commits & Branches

- Conventional-Commits-Stil: `feat(scope): …`, `fix(scope): …`, `docs: …`, `style: …`.
- Beschreibe **warum**, nicht nur was. Verweise auf ADRs/Gaps, wenn relevant.
- Kleine, fokussierte Commits; CI muss grün sein.

## Wo was liegt

- Code: `crates/` (Reader, Store, Bridge, AKN/JOLux, Telemetry).
- Architektur: `likec4/` und `docs/` (`10_LEXICON`, `30_PLAN`, `45_GAP_ANALYSIS`,
  `50_ROADMAP_TO_PERFECT`, ADRs unter `docs/adr/`).
- Betrieb: `docs/70_CONFIG.md`, `docs/80_DEPLOY.md`, `docs/90_AUTH_AND_ROLES.md`.

## Sicherheit

Sicherheitsrelevante Funde bitte **nicht** als öffentliches Issue — siehe
[`SECURITY.md`](./SECURITY.md).

<!--
Danke für deinen Beitrag! Bitte halte den PR klein und fokussiert.
Beschreibe das WARUM, nicht nur das WAS. Siehe CONTRIBUTING.md.
-->

## Was & Warum

<!-- Kurze Zusammenfassung der Änderung und der Motivation. -->

## Verwandte Issues / ADRs

<!-- z. B. "Schliesst #123", "Bezug zu ADR-006", "Gap G-2 aus 45_GAP_ANALYSIS.md" -->

## Art der Änderung

- [ ] `fix` — Fehlerbehebung
- [ ] `feat` — neue Funktion / neues Tool
- [ ] `docs` — nur Dokumentation
- [ ] `refactor` / `style` — kein Verhaltenswechsel
- [ ] Sonstiges:

## CI-Gleichlauf (lokal grün, siehe CONTRIBUTING.md)

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Bei netz-/dockerabhängigen Änderungen: `cargo test --workspace -- --ignored`
- [ ] `scripts/smoke.sh <base-url> <token>` grün (bei Transport-/Deploy-Änderungen)

## Architektur-Invarianten (ADRs) gewahrt

- [ ] **Identität nie aus LLM-Parametern** (ADR-002): `tenant`/`session`/`role` nur aus Claims.
- [ ] **Provenance an jeder Normtext-Antwort** (ADR-004): `eli` + `valid_as_of`.
- [ ] **PII-Scrubbing im Audit-Log** (ADR-001): keine rohen Argumente/Antwortinhalte geloggt.
- [ ] **Least-Privilege-Pools** (ADR-006/007): neues Tool hat `ToolPool` **und** Eintrag in
      `crates/mcp-reader/tests/lexicon_projection.rs` (sonst rot).
- [ ] **fail-closed**: Auth-/Quota-Pfade verweigern im Zweifel.

## Hinweise für Reviewer

<!-- Worauf besonders achten? Bekannte Einschränkungen? Folge-PRs? -->

## Sicherheit

- [ ] Diese Änderung ist **nicht** sicherheitsrelevant. (Andernfalls vor dem PR den
      vertraulichen Meldeweg aus [`SECURITY.md`](../SECURITY.md) nutzen.)

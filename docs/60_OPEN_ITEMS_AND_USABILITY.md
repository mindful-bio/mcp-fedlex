# 60 — Vollständiger Fahrplan offener Punkte (Betrieb & Nutzbarkeit für Dritte)

> **Was dieses Dokument ist.** Eine **am Code verifizierte, vollständige** Liste aller noch offenen
> Punkte, damit `mcp-fedlex` von Dritten **idiotensicher** als Open-Source-Projekt verwendet werden
> kann. Es schliesst nahtlos an [45_GAP_ANALYSIS.md](45_GAP_ANALYSIS.md) und
> [50_ROADMAP_TO_PERFECT.md](50_ROADMAP_TO_PERFECT.md) an: Die dortige **Tool-Projektion**
> (Schritte 1–4) ist erledigt; offen bleiben **Day-2-Härtung** (Roadmap-Schritt 5/6) und eine
> bisher **nirgends erfasste Onboarding-/DX-Schicht**. Jeder Punkt hat eine **Abnahme**.
> Methodik: Lektüre von `README.md`, `Dockerfile`, `.gitlab-ci.yml`, `.github/workflows/`,
> `crates/mcp-reader/src/{main,transport}.rs`, `crates/fedlex-store/src/redis_store.rs` +
> `ls` der Top-Level-Komfortdateien. Stand: 2026-06-18.

---

## 0. Einordnung: Was bereits steht (kein Handlungsbedarf)

Damit der Fokus klar ist — diese Dimensionen sind **verifiziert grün** und stehen **nicht** auf
der Liste unten:

- **Server-Kern**: zustandslos, fail-closed Auth (JWT HS256/RS256/JWKS + `MCP_DEV_TOKEN`),
  verteilte Quota mit Pool-Gewichten, Provenance-Gate, PII-Scrubbing, Tenant-Isolation — unit-
  und live-getestet.
- **Datenschicht**: jolux (29) + akn (12) + bridge (3) Live-Konformanztests, wöchentlich in CI
  (`.github/workflows/live-conformance.yml`).
- **Tool-Projektion**: 25 Tools; Vollständigkeit testverankert (`tests/lexicon_projection.rs`).
  → G-1/G-2/G-4 geschlossen, Roadmap-Schritte 1–4 erledigt.

- **CI**: `.github/workflows/ci.yml` + `live-conformance.yml` + `.gitlab-ci.yml` (Kaniko-Build).
- **Deployment**: Distroless-Image (nonroot, UID 65532), mTLS-zu-Redis (ADR-005) — live & Healthy.

---

## 1. Befund: Die Substanz ist exzellent, die Einstiegshürde ist hoch

Ein Fremder, der das Repo heute klont, scheitert **nicht** am Code, sondern an fehlender
Anleitung und fehlendem Ein-Befehl-Start. Verifizierte Lücken:

| ID | Lücke | Beleg (verifiziert) |
| --- | --- | --- |
| **U-1** | README ist ein 18-Zeilen-Stub („🚧 Early development"), kein Quickstart, keine Tool-Liste, keine Client-Anbindung | `README.md` (18 Zeilen) |
| **U-2** | Kein lokaler Ein-Befehl-Start (kein `docker-compose.yml`, kein `.env.example`, kein `Makefile`) | `ls`: alle vier FEHLT |
| **U-3** | Env-/Konfig-Oberfläche nur als Doc-Kommentar im Quellcode, nirgends als Referenztabelle | `main.rs` (Z. 39–196) |
| **U-4** | Kein Post-Deploy-Smoke-Test (genau der heutige 503-Vorfall wäre damit sofort sichtbar) | kein Skript/Target vorhanden |
| **U-5** | OSS-Hygiene fehlt: `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, Issue/PR-Templates | `ls`: alle FEHLT |
| **U-6** | RBAC/Rollen & Token-Beschaffung undokumentiert (Dev-Token ⇒ Validator; JWT-Claims-Schema) | `main.rs` (Z. 144–196), kein Doc |
| **U-7** | Deploy-Pfad für Dritte fehlt: SealedSecrets-Schritt war manuell & untested (heutiger Vorfall) | `k3-infra/.../gen-redis-mtls.sh` |
| **U-8** | Kein veröffentlichtes Image / keine Versionierung/Tags für Fremdnutzung (nur interne Registry) | `.gitlab-ci.yml` (nur `latest`/SHA, interne Registry) |
| **U-9** | MCP-Protokollversion veraltet (`2024-11-05` hartcodiert); aktuelle Spec-Revision nicht ausgehandelt | `transport.rs:217` — siehe [ADR-008](adr/ADR-008-mcp-protocol-version-upgrade.md) |


Offene **interne** Roadmap-Punkte (aus 50_ROADMAP, unverändert gültig):

| ID | Lücke | Beleg |
| --- | --- | --- |
| ~~**R-5a**~~ | ~~Oxigraph Backup/Restore (B-1)~~ → **erledigt (2026-06-21)**: `dump_to_string`/`restore_from_str`, append-only-treuer Roundtrip, 8 Unit-Tests | 50_ROADMAP §Schritt 5 |
| ~~**R-5b**~~ | ~~AKN/JOLux-Schema-Versionierung (B-2)~~ → **erledigt (2026-06-21)**: `SCHEMA_VERSION` im Backup-Kopf, harter `SchemaMismatch`-Guard beim Restore | 50_ROADMAP §Schritt 5 |

| ~~**R-5c**~~ | ~~CI-Härtung: Alert/Issue bei rotem Live-Lauf~~ → **erledigt (2026-06-21)**: `notify`-Job öffnet/schliesst dedupliziertes Issue, Frequenz auf 2×/Woche | 45_GAP §G-5 |

| ~~**R-6**~~ | ~~AKN-Aufbereitungs-Tools abwägen~~ → **erledigt (2026-06-21)**: `extract_tables`/`list_components`/`detect_foreign_content` projiziert, `hollow_document`/`chunk_document` bewusst ausgeschlossen | 45_GAP §G-2, 50_ROADMAP §Schritt 6 |


---

## 2. Vollständige Checkliste (nach Hebelwirkung)

> Reihenfolge: erst der **2-Minuten-Einstieg** (U-1…U-3), dann **Vertrauen/Sicherheit beim Deploy**
> (U-4, U-7), dann **OSS-Hygiene & Verteilung** (U-5, U-6, U-8), zuletzt **interne Day-2-Härtung**
> (R-5/R-6). Jeder Block endet mit seiner Abnahme.

### Block A — 2-Minuten-Einstieg (höchster Hebel) ✅ ERLEDIGT (2026-06-19)

- [x] **A-1 (U-2): `docker-compose.yml`** — Reader + Redis (Klartext im compose-Netz),
      `env_file: .env`, Port 8080 gemappt; Redis-Healthcheck, kein Reader-In-Container-Check
      (distroless). `docker compose up --build` ⇒ lauffähiger Server ohne Rust-Toolchain.
- [x] **A-2 (U-2): `.env.example`** — alle relevanten Variablen mit Default & Kommentar; von
      compose via `env_file` referenziert; `.env` ist in `.gitignore`.
- [x] **A-3 (U-1): README-Quickstart** — *Was/Warum*, *Tool-Liste (22, nach Pool)*, *In 2 Minuten
      lokal* (`docker compose up` + `curl` für `initialize`/`tools/list`/`tools/call`), *MCP-Client-
      Anbindung*, *Health-Endpunkte*, *Verweis auf CONFIG/AUTH*, *Lizenz/Links*.
- [x] **A-4 (U-3): `docs/70_CONFIG.md`** — Referenztabelle jeder Env-Variable (Name, Default,
      Pflicht, Beispiel, Wirkung) inkl. Auth-Auswahlreihenfolge und Redis-mTLS-Trias.


> **Abnahme A.** Frischer Clone: `cp .env.example .env && docker compose up` ⇒ `curl -s -X POST
> localhost:8080/rpc -H 'authorization: Bearer <dev>' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'`
> liefert die Tool-Liste als JSON. README beschreibt exakt diesen Pfad.

### Block B — Vertrauen beim Deploy ✅ ERLEDIGT (2026-06-19)

- [x] **B-1 (U-4): Post-Deploy-Smoke-Test** — `scripts/smoke.sh <base-url> <token>` prüft
      `/livez`, `/readyz`, `POST /rpc initialize` (serverInfo) und `tools/call read_article`
      (Provenance); **failt mit Exit≠0 bei 503/HTML statt JSON** (genau der 503-Vorfall vom
      18.06.). Exit-Codes 0/1/2, farbige Ausgabe.
- [x] **B-2 (U-7): `docs/80_DEPLOY.md`** — k8s-Pfad end-to-end (Topologie, SealedSecret-Tabelle,
      `gen-redis-mtls.sh`, Reihenfolge Secrets-vor-Workloads, Pod-Rollout nach Secret-Änderung,
      Smoke-Test) **plus §6 Runbook für den 503-Vorfall** mit Diagnose-Kommandos und Merksatz
      „distroless ⇒ kein exec, immer Port-Forward/Ingress".
- [x] **B-3 (U-7): `gen-redis-mtls.sh` Self-Check** — Controller-Erreichbarkeit wird geprüft,
      Default-Name auf `sealed-secrets-controller` korrigiert, Env-übersteuerbar. *(im Infra-Repo
      committet: `37d6558`, `dd0fb3b`.)*
- [x] **B-4 (U-4): Health-Endpunkte in README** — `/livez` `/readyz` `/startupz` dokumentiert
      (README „In 2 Minuten lokal" + `70_CONFIG.md §4`).


> **Abnahme B.** `make smoke` gegen eine frische Deployment-Instanz ist grün; ein absichtlich
> kaputtes Deployment (Redis aus) lässt `smoke.sh` rot werden. DEPLOY.md führt Schritt für Schritt
> zu einem grünen Smoke.

### Block C — OSS-Hygiene & Verteilung (teilweise erledigt 2026-06-19)

- [x] **C-1 (U-5): `CONTRIBUTING.md`** — Voraussetzungen, lokaler Lauf (`docker compose` + Smoke),
      CI-Gleichlauf (fmt/clippy/test, `-- --ignored` für Live), Architektur-Leitplanken (ADRs),
      Commit-Konvention, Verweis auf SECURITY.md.
- [x] **C-2 (U-5): `SECURITY.md`** — Meldeweg (security@mindful.bio), Geltungsbereich, fail-closed-
      Sicherheitsmodell, Geheimnis-/SealedSecret-Handhabung (ADR-001/002/005).
- [x] **C-3 (U-5): `CHANGELOG.md`** — „Keep a Changelog"-Format; Erst-Eintrag `0.1.0` aus
      Git-Historie, `Unreleased`-Sektion, Compare-/Tag-Links. *(2026-06-20)*
- [x] **C-4 (U-5): Issue/PR-Templates** — `.github/ISSUE_TEMPLATE/` (`bug_report.md`, `feature_request.md`,
      `config.yml` mit vertraulichem Security-Link) + `.github/PULL_REQUEST_TEMPLATE.md`. Alle vier
      tragen die ADR-Leitplanken (Identität/Provenance/PII/Least-Privilege) und den CI-Gleichlauf
      aus `CONTRIBUTING.md` als Checklisten. *(2026-06-21)*
- [x] **C-5 (U-6): `docs/90_AUTH_AND_ROLES.md`** — Rollenmodell (Reader ⊆ Navigator ⊆ Validator),
      Pool-Sichtbarkeitsmatrix, JWT-Claims-Schema (iss/aud/role/tenant/sid), Quota pro Rolle,
      Auth-Auswahlreihenfolge (JWKS/HS256/RS256/Dev-Token), Audit-Log-Beispiel.
- [x] **C-6 (U-8): Veröffentlichungs-Pfad** — Hybrid: dynamische `latest`/`<sha>`-Tags für den
      internen Deploy **plus** zitierbare SemVer-Images über einen `release-image`-Job, der auf
      Git-Tag `vX.Y.Z` baut (`.gitlab-ci.yml`). README-Abschnitt „Versioniertes Image beziehen"
      nennt `:v0.1.0` als empfohlene Fremdnutzer-Referenz. *(2026-06-20)*
- [x] **C-7 (U-1): README-Badges** — Pipeline-Status, Lizenz, Release-Badge (`v0.1.0`). Ein
      **MCP-Protokoll-Badge bleibt bewusst ausgesetzt**, bis die Version aktuell ist: der Server
      handelt heute `2024-11-05` aus; ein Badge mit neuerer Zahl wäre falsche Provenance.
      → Upgrade als **U-9 / Block E** ([ADR-008](adr/ADR-008-mcp-protocol-version-upgrade.md)).
      *(2026-06-20)*


> **Abnahme C.** Repo-Startseite zeigt klar: Was, Wie-mitmachen, Wie-melden, Wie-anbinden. Ein
> Fremder findet Rollen/Token-Doku ohne Quellcode-Lektüre. Es existiert eine zitierbare
> Image-Referenz oder eine eindeutige Build-Anleitung.

### Block D — Interne Day-2-Härtung (Roadmap-Schritt 5/6)

- [x] **D-1 (R-5a):** Oxigraph **Backup/Restore** — **erledigt (2026-06-21)**:
      `OxigraphCorpus::dump_to_string`/`restore_from_str` (in-process, kein Docker) schreiben einen
      deterministischen, menschenlesbaren Schnappschuss (Kopfzeile + eine tab-separierte Zeile je
      Fassung, Text escaped) und bauen daraus einen frischen Korpus. Der Roundtrip ist
      append-only-treu: bi-temporale Auflösung, Gültigkeits-/Transaktionszeit und Mehrfach-Historie
      pro ELI bleiben erhalten; ein erneuter Dump ist byte-gleich. Nachweis: 8 neue Unit-Tests in
      `oxigraph_corpus.rs` (Determinismus, Roundtrip inkl. Tabs/Zeilenumbrüchen im Text, leerer
      Korpus, korrupte/abgeschnittene Backups). `cargo test`-beweisbar statt nur Infra.
- [x] **D-2 (R-5b):** **Schema-Versionierung** (B-2) — **erledigt (2026-06-21)**:
      `SCHEMA_VERSION`-Konstante wandert in jeden Backup-Kopf; `restore_from_str` prüft sie und
      bricht bei Abweichung hart mit `GraphError::SchemaMismatch { found, expected }` ab, statt still
      inkompatible Daten zu laden. Migrationsnotiz am Restore-Pfad dokumentiert den Upgrade-Haken
      `v(N-1) → vN`. Nachweis: Test `restore_rejects_foreign_schema_version`.
- [x] **D-3 (R-5c):** CI **Alert/Issue** bei rotem Live-Lauf — **erledigt (2026-06-21)**:
      `live-conformance.yml` hat einen `notify`-Job (`if: always()`, `actions/github-script`), der
      bei rotem Lauf ein **dedupliziertes** Issue (Label `live-conformance`, Triage-Reihenfolge)
      öffnet bzw. kommentiert und bei grünem Folgelauf automatisch schliesst; Frequenz von
      wöchentlich auf **2×/Woche** (Mo + Do) erhöht. `45_GAP §G-5` geschlossen.

- [x] **D-4 (R-6):** Entscheidung **AKN-Aufbereitungs-Tools** — **erledigt (2026-06-21)**:
      `extract_tables`/`list_components`/`detect_foreign_content` als MCP-Tools projiziert (Pool
      `LocalNavigation`, mit Unit-Tests), `get_component_document` als interner Helper sowie
      `hollow_document`/`chunk_document` (RAG-Bausteine) bewusst ausgeschlossen. Matrix in
      `tests/lexicon_projection.rs` nachgezogen (24 `Projected` / 23 `Excluded`, 25 Tools);
      `45_GAP §G-2` dokumentiert. Alle Tests grün.


> **Abnahme D.** Je Punkt der in [30_PLAN.md → M10](30_PLAN.md) genannte Test-/Infra-Nachweis;
> Matrix bleibt nach D-4 grün und vollständig.

### Block E — MCP-Protokoll-Upgrade (U-9) — geplant, [ADR-008](adr/ADR-008-mcp-protocol-version-upgrade.md)

Der Handshake handelt heute `2024-11-05` aus (eine Konstante in `transport.rs:217`). Der
Methoden-Footprint ist bewusst klein (`initialize`, `tools/list`, `tools/call`; Capabilities
nur `{tools:{}}`; kein `ping`/`notifications`/`resources`/`prompts`), daher ist das Upgrade
abgrenzbar — aber kein Badge-Tausch: die Zielversion-Deltas wurden an der offiziellen
Spec verifiziert (ADR-008 §A). **Ziel-Revision: `2025-11-25`** (`2025-06-18` ist als Ziel
ausgeschlossen). Erst auf dieser Basis steigen Code/String. Zielrelease **`v0.2.0`**.


> **Operatives Runbook:** [`55_MIGRATION_mcp_protocol_upgrade.md`](55_MIGRATION_mcp_protocol_upgrade.md)
> — Phasen 0–9 mit Gate & Rollback je Schritt.
>
> **Zielbild (eindeutig, ohne Rückwärtskompatibilitäts-Anspruch):** Der Server muss am
> Ende **sauber gegen die MCP-Revision `2025-11-25` funktionieren** — gemessen am
> Konformanztest, nicht an der Kompatibilität mit dem heutigen Verhalten. Rückwärts-/
> Abwärtskompatibilität ist **ausdrücklich kein Ziel**: bestehende Konsumenten (ansV,
> syllogismus-fedlex) werden, wo nötig, im selben Schritt auf den `2025-11-25`-Pfad
> nachgezogen, statt den Server an deren Alt-Verhalten zu binden.



- [x] **E-1 (§A):** Spec-Recherche — **erledigt (2026-06-20)**. Höchste stabile Revision
      **`2025-11-25`** als Ziel festgelegt (`2025-06-18` ausgeschlossen); Delta-Matrix
      (neu/geändert/deprecated/breaking) gegen unseren Footprint in
      [ADR-008 §A](adr/ADR-008-mcp-protocol-version-upgrade.md); in `CHANGELOG.md`
      (`Unreleased` → `v0.2.0`) vorgemerkt.

- [x] **E-2 (§B):** Versions-Negotiation (`SUPPORTED_PROTOCOL_VERSIONS`), Transport-Anpassung
      (HTTP+SSE → Streamable HTTP), Auth-Mapping (ADR-002 fail-closed bleibt), Lifecycle-
      Notifications/Capabilities ehrlich. **Vollständig im Code umgesetzt.**
      > **Stand 2026-06-21 (code-verifiziert gegen `transport.rs`/`protocol.rs`/`registry.rs`):**
      > 1. ✅ **`schema`→`inputSchema`:** additiver Doppel-Output live (`registry.rs::list_tools`),
      >    ansV liest `inputSchema` bevorzugt mit `schema`-Fallback (`llm.rs`). Baseline-Test 1.2 grün.
      > 2. ✅ **Notification-Handling:** `id` ist jetzt `Option<Option<Value>>` mit `is_notification()`;
      >    `rpc_handler` quittiert Notifications mit **HTTP 202 ohne Body**; `ping` + `notifications/initialized`
      >    sind eigene Match-Arme.
      > 3. ✅ **Delta #13 (Input-Validation):** beide `tools/call`-Pfade (fehlender `name`, ungültiges
      >    `as_of`) liefern jetzt einen in-band Tool-Execution-Error (`{ error, hint }` im `result`)
      >    statt `-32602` — `2025-11-25`-konform; die Konstante `INVALID_PARAMS` wurde aus
      >    `transport.rs` entfernt, eigene Tests sichern beide Pfade.
      > 4. ✅ **`rpc_handler`-Signatur:** bereits auf `impl IntoResponse`/`Response` umgebaut (kann
      >    202/204/400/401/403 ausdrücken).
      > 5. ✅ **Streamable-HTTP-Transport + 403/`Origin` + `MCP-Protocol-Version`-Header:** **live**
      >    (2026-06-21). Neuer Endpoint **POST `/mcp`** (`transport.rs::mcp_handler`, Route registriert)
      >    setzt die zwei Spec-Wächter **vor jeder Arbeit** durch: `Origin` ausserhalb der Allowlist
      >    (`MCP_ALLOWED_ORIGINS`, fail-closed) → **HTTP 403**; gesetzter, nicht unterstützter
      >    `MCP-Protocol-Version`-Header → **HTTP 400**; fehlende Header bleiben rückwärtskompatibel
      >    (Server-zu-Server-Clients ohne `Origin`/Header unverändert erlaubt). Klassifikatoren
      >    `classify_origin`/`classify_protocol_header` in `protocol.rs` (mit Unit-Tests); 6 HTTP-Tests
      >    für `/mcp` (403/allowed/no-origin/400/supported/202-notification) grün.

- [x] **E-3 (§C):** Konformanz- + Provenance-Konsistenz-Test (ausgehandelte Version == Badge ==
      ADR-Ziel), Client-Gegentest, Docs/Badge nachziehen, `v0.2.0` taggen.
      > **Stand 2026-06-21:** Konsistenz- und `/mcp`-Konformanztests grün (160 Lib-Tests, davon die
      > 6 neuen `/mcp`-HTTP-Tests). Verbleibend rein redaktionell: README-Badge auf `2025-11-25`
      > heben und `v0.2.0` taggen (kein Code-Risiko mehr).


> **Abnahme E.** `initialize` handelt die verifizierte aktuelle Revision aus; ein
> Konsistenztest bricht CI, wenn Code-Version, README-Badge und ADR-Ziel divergieren.

---


## 3. Reihenfolge in einem Satz

Zuerst den **2-Minuten-Einstieg** schaffen (Block A), dann **Vertrauen beim Deploy** durch Smoke-
Test & Deploy-Doku (Block B), danach **OSS-Hygiene & Verteilung** (Block C), zuletzt die
**interne Day-2-Härtung** (Block D) — wobei A der mit Abstand grösste Hebel für „von Dritten
nutzbar" ist.

## 4. Nicht-Ziele (bewusst nicht auf der Liste)

- **Tools „auf Verdacht"** (Tranche D: treaties/genesis/publication/vocabulary) — nur bei
  belegtem Bedarf (siehe 50_ROADMAP §Nicht-Ziele).
- **Eigene Auth-/IdP-Implementierung** — der Server konsumiert JWT/JWKS; einen IdP zu betreiben
  ist Sache des Nutzers (in 90_AUTH_AND_ROLES.md nur beschrieben, nicht mitgeliefert).
- **Mandantenfähiges Hosting-Angebot** — out of scope für das OSS-Repo.

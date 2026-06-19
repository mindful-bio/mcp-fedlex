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
- **Tool-Projektion**: 22 Tools; Vollständigkeit testverankert (`tests/lexicon_projection.rs`).
  → G-1/G-4 geschlossen, Roadmap-Schritte 1–4 erledigt.
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

Offene **interne** Roadmap-Punkte (aus 50_ROADMAP, unverändert gültig):

| ID | Lücke | Beleg |
| --- | --- | --- |
| **R-5a** | Oxigraph Backup/Restore (B-1) als nachweisbare Eigenschaft | 50_ROADMAP §Schritt 5 |
| **R-5b** | AKN/JOLux-Schema-Versionierung (B-2) | 50_ROADMAP §Schritt 5 |
| **R-5c** | CI-Härtung: Alert/Issue bei rotem Live-Lauf (G-5) | 45_GAP §G-5 |
| **R-6** | AKN-Aufbereitungs-Tools abwägen: `extract_tables`/`list_components`/`detect_foreign_content` projizieren vs. `hollow_document`/`chunk_document` bewusst ausschliessen | 45_GAP §G-2, 50_ROADMAP §Schritt 6 |

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
- [ ] **C-3 (U-5): `CHANGELOG.md`** — „Keep a Changelog"-Format; erste Einträge aus Git-Historie.
- [ ] **C-4 (U-5): Issue/PR-Templates** — `.github/ISSUE_TEMPLATE/` (bug/feature) + `PULL_REQUEST_TEMPLATE.md`.
- [x] **C-5 (U-6): `docs/90_AUTH_AND_ROLES.md`** — Rollenmodell (Reader ⊆ Navigator ⊆ Validator),
      Pool-Sichtbarkeitsmatrix, JWT-Claims-Schema (iss/aud/role/tenant/sid), Quota pro Rolle,
      Auth-Auswahlreihenfolge (JWKS/HS256/RS256/Dev-Token), Audit-Log-Beispiel.
- [ ] **C-6 (U-8): Veröffentlichungs-Pfad** — entweder GHCR-Push im GitHub-Workflow **oder** klar

      dokumentierter Eigenbau (`docker build`). Mindestens **SemVer-Tag** zusätzlich zu
      `latest`/SHA; README nennt das nutzbare Image-Referenz.
- [ ] **C-7 (U-1): README-Badges** — CI-Status, Lizenz, MCP-Protokollversion (2024-11-05).

> **Abnahme C.** Repo-Startseite zeigt klar: Was, Wie-mitmachen, Wie-melden, Wie-anbinden. Ein
> Fremder findet Rollen/Token-Doku ohne Quellcode-Lektüre. Es existiert eine zitierbare
> Image-Referenz oder eine eindeutige Build-Anleitung.

### Block D — Interne Day-2-Härtung (Roadmap-Schritt 5/6)

- [ ] **D-1 (R-5a):** Oxigraph **Backup/Restore** als nachweisbare Prozedur + Test/Runbook (B-1).
- [ ] **D-2 (R-5b):** **Schema-Versionierung** AKN/JOLux (B-2) — Versionsmarke + Migrationsnotiz.
- [ ] **D-3 (R-5c):** CI **Alert/Issue** bei rotem Live-Lauf (ggf. häufigere Frequenz als wöchentlich).
- [ ] **D-4 (R-6):** Entscheidung **AKN-Aufbereitungs-Tools**: `extract_tables`/`list_components`/
      `detect_foreign_content` projizieren (nutzwertig) vs. `hollow_document`/`chunk_document`
      bewusst ausschliessen; Ergebnis in `tests/lexicon_projection.rs`-Matrix nachziehen.

> **Abnahme D.** Je Punkt der in [30_PLAN.md → M10](30_PLAN.md) genannte Test-/Infra-Nachweis;
> Matrix bleibt nach D-4 grün und vollständig.

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

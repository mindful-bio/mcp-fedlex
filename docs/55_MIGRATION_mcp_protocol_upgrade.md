# 55 — Migrations-Runbook: MCP-Protokoll-Upgrade (extrem vorsichtig)

> **Zweck.** Schritt-für-Schritt-Ausführungsplan, um `mcp-fedlex` **ohne Bruch** von der heute
> ausgehandelten Protokollrevision `2024-11-05` auf die aktuelle MCP-Spec zu heben. Dieses Dokument
> ist die **operative Ergänzung** zur Entscheidung in
> [ADR-008](adr/ADR-008-mcp-protocol-version-upgrade.md): ADR-008 sagt *warum* und *was*, dieses
> Runbook sagt *wie genau*, *in welcher Reihenfolge*, *wie geprüft* und *wie zurückgerollt*.
>
> **Leitsatz.** Kein Schritt geht live, bevor sein Verifikations-Gate grün ist. Jede Phase hat
> einen definierten **Rollback**. Provenance bleibt ehrlich: ausgehandelte Version == Badge ==
> ADR-Ziel == Test — sonst bricht CI.

---

## 0. Warum „extrem vorsichtig" hier berechtigt ist

Der Reader ist **produktiv** (`https://mcp-fedlex.ch`) und hat **mindestens einen echten
Konsumenten in Produktion**: die ansV-Plattform (`ansv-fedlex::McpClient`). Ein unsauberes
Upgrade bricht nicht nur den Server, sondern die belegbare Gutachten-Kette von ansV.

**Verifizierter Ist-Zustand (Code-belegt):**

| Seite | Fakt | Beleg |
| --- | --- | --- |
| Server | Protokollversion hartcodiert `2024-11-05` | `crates/mcp-reader/src/transport.rs:217` |
| Server | Methoden: `initialize`, `tools/list`, `tools/call` | `transport.rs` (`match req.method`) |
| Server | Capabilities nur `{ "tools": {} }`; kein `ping`/`notifications`/`resources`/`prompts`/`logging` | `transport.rs` |
| Server | Transport: `GET /sse` (nennt POST-Endpoint), `POST /rpc` | `transport.rs` (`sse_handler`/`rpc_handler`) |
| Server | Auth: Bearer/JWT pro Request, fail-closed | ADR-002, `transport.rs` (`auth.verify`) |
| **Konsument** | ansV `McpClient` ruft **direkt** `POST {base}/rpc` mit `tools/list`/`tools/call` | `ansV/crates/ansv-fedlex/src/client.rs` |
| **Konsument** | ansV ruft **kein `initialize`** — handelt also heute **keine Version aus** | `client.rs` (nur `rpc("tools/list"/"tools/call")`) |
| Konsument | Live-Test gegen `https://mcp-fedlex.ch`, gated durch `MCP_FEDLEX_JWT` | `ansv-fedlex/tests/live.rs` |
| Konsument | E2E-Mock bildet `/rpc` mit `tools/list`+`tools/call` nach | `ansv-fedlex/tests/e2e_belegkette.rs` |
| Infra | Öffentlich via Ingress `mcp-fedlex.ch` | `k3-infra/.../mcp-fedlex/ingress.yaml` |

**Daraus folgt die zentrale Migrationsregel:**
> Weil der einzige bekannte Konsument heute **ohne `initialize`** und **direkt auf `/rpc`** arbeitet,
> ist jede Änderung an Transport-Pfad oder Pflicht-Handshake **breaking**, solange der alte Pfad
> nicht parallel bestehen bleibt. Die Migration ist deshalb **additiv und rückwärtskompatibel**
> auszulegen, niemals als harter Schnitt.

---

## Phase 0 — Vorbereitung & Faktenbasis (kein Code)

**Ziel:** Vollständige, verifizierte Grundlage. Keine Annahme bleibt ungeprüft.

- [ ] **0.1 Spec-Quelle fixieren.** Offizielle Revisionsliste + Schema von
      `modelcontextprotocol.io` (Specification) und dem `modelcontextprotocol/modelcontextprotocol`-
      Repo (`schema/`). **Exakte Ziel-Revision** und ihren Commit/Datum notieren.
- [ ] **0.2 Delta-Matrix erstellen.** Pro Revision **ab** `2024-11-05` **bis** Ziel: jede Änderung
      als Zeile mit Klassifikation `NEU | GEÄNDERT | DEPRECATED | BREAKING`. Tabelle in ADR-008 §A-2
      übernehmen (Single Source of Truth der Fakten).
- [ ] **0.3 Betroffenheits-Mapping.** Jede Delta-Zeile gegen die Ist-Tabelle (§0 oben) prüfen:
      `betrifft uns | optional | irrelevant`. Besonders markieren: Transport, Auth, Lifecycle,
      `initialize`-Antwortform.
- [ ] **0.4 Konsumenten-Inventar abschliessen.** Bestätigen, dass ansV der **einzige**
      Produktions-Client ist (grep nach `mcp-fedlex.ch` / `McpClient` über alle Repos). Jeden
      weiteren Client hier eintragen. *(Heute bekannt: nur ansV.)*
- [ ] **0.5 Abbruchkriterien definieren.** Schwellen, ab denen die Migration pausiert (z. B.
      Live-Konformanz rot, ansV-E2E rot, Quota-/Auth-Regression).

**Gate 0:** ADR-008 §A-1…A-4 ausgefüllt, Delta-Matrix reviewt, Ziel-Revision schriftlich fixiert.
**Rollback:** entfällt (reine Doku).

---

## Phase 1 — Sicherheitsnetz VOR jeder Code-Änderung

**Ziel:** Regressionen werden mechanisch sichtbar, bevor wir irgendetwas anfassen.

- [ ] **1.1 Baseline-Konformanztest (alt).** Test, der den **heutigen** `initialize`-Handshake
      festschreibt (`protocolVersion == "2024-11-05"`, `capabilities.tools` vorhanden,
      `serverInfo.name`). Muss **jetzt grün** sein. (Neu unter `crates/mcp-reader/tests/`.)
- [ ] **1.2 Methoden-Snapshot.** Test, der die exakte Antwortform von `tools/list` und einem
      `tools/call` (inkl. `provenance`-Block) als Snapshot fixiert — schützt den Konsumenten-Vertrag.
- [ ] **1.3 ansV-Vertragstest grün stellen.** `cargo test -p ansv-fedlex` (Unit + E2E-Mock
      `e2e_belegkette.rs`) lokal grün; `live.rs` einmal mit gültigem `MCP_FEDLEX_JWT` gegen die
      **aktuelle** Prod-Instanz laufen lassen und Ergebnis archivieren (Referenz-Lauf).
- [ ] **1.4 Smoke-Baseline.** `scripts/smoke.sh https://mcp-fedlex.ch <token>` grün dokumentieren.
- [ ] **1.5 Image-Pin für Rollback.** Aktuelle Prod-Image-Referenz (`:<sha>`/`:v0.1.0`) notieren,
      damit ein Rollback ein exakter Tausch ist.

**Gate 1:** Alle Baseline-Tests grün und als „Vorher-Stand" archiviert.
**Rollback:** entfällt (nur additive Tests).

---

## Phase 2 — Versions-Negotiation (additiv, noch kein Verhalten geändert)

**Ziel:** Der Server *kann* mehrere Versionen, verhält sich aber für Alt-Clients identisch.

- [ ] **2.1 `SUPPORTED_PROTOCOL_VERSIONS`** als sortierte Konstante einführen (älteste …
      Ziel-Revision). Single Source of Truth für Handshake **und** Konsistenztest.
- [ ] **2.2 `initialize`-Negotiation.** Client-`protocolVersion` aus den Params lesen:
      - bekannt & gemeinsam → diese aushandeln;
      - **fehlt** (heutiger ansV-Fall!) → bewusst definierte Default-Version aushandeln
        (zunächst weiterhin `2024-11-05`, damit Alt-Clients unverändert laufen);
      - unbekannt/zu neu → höchste eigene anbieten (Spec-konforme Antwort, kein harter Fehler).
- [ ] **2.3 Default-Version als Config.** Per Env steuerbar (z. B. `MCP_PROTOCOL_DEFAULT`), damit
      der Sprung der Default-Version später ein **Config-Flip ohne Redeploy-Code** ist.
- [ ] **2.4 Tests.** Negotiation-Matrix (fehlend/bekannt/unbekannt) als Unit-Test; Baseline 1.1
      bleibt grün (Default unverändert).

**Gate 2:** Negotiation-Tests grün; Baseline 1.1–1.4 weiterhin grün; ansV-E2E unverändert grün.
**Rollback:** Feature ist additiv; Default unverändert ⇒ Risiko ~0. Notfalls Commit revert.

---

## Phase 3 — Transport-Kompatibilität (der riskanteste Teil)

**Ziel:** Falls die Ziel-Revision den HTTP+SSE-Weg deprecatet (→ „Streamable HTTP"), neuen Weg
**zusätzlich** anbieten, alten Weg erhalten.

- [ ] **3.1 Entscheidung aus Delta-Matrix.** Nur wenn 0.3 „betrifft uns" ergab. Sonst Phase
      überspringen und dokumentieren.
- [ ] **3.2 Neuen Transport additiv implementieren.** Neuer Endpoint/Modus parallel zu `/sse`+
      `/rpc`. **`/rpc` bleibt unverändert bestehen**, solange ein Alt-Client existiert (ansV).
- [ ] **3.3 Health unberührt.** `/livez` `/readyz` `/startupz` bleiben wie sind (`health.rs`).
- [ ] **3.4 Doppelpfad-Tests.** Beide Transporte gegen denselben Tool-Aufruf; identische
      `provenance`-Ausgabe. Snapshot 1.2 muss auf beiden Wegen passen.
- [ ] **3.5 Lasttest/Quota.** Sicherstellen, dass der neue Pfad dieselbe Quota-/Auth-Kette
      durchläuft (ADR-002), kein Bypass.

**Gate 3:** Beide Transporte grün, identische Antworten, kein Auth-/Quota-Bypass.
**Rollback:** Neuen Pfad per Feature-Flag/Route deaktivieren; `/rpc` trägt weiter.

---

## Phase 4 — Auth-Mapping

**Ziel:** Auth-Modell der Ziel-Revision (z. B. OAuth2-Resource-Server) gegen unser Bearer/JWT
abgleichen, **ohne** fail-closed aufzuweichen.

- [ ] **4.1 Gap-Analyse Auth.** Verlangt die Ziel-Revision Metadaten (z. B. WWW-Authenticate,
      Resource-Indicators)? Pflicht vs. optional aus Delta-Matrix.
- [ ] **4.2 Additiv ergänzen.** Fehlende Pflicht-Header/Discovery-Dokumente bereitstellen;
      bestehende JWT/JWKS-Kette (ADR-002) bleibt gültig.
- [ ] **4.3 Negativtests.** Fehlendes/abgelaufenes/falsches Token weiterhin hart abgelehnt;
      Identität nie aus Params (ADR-002-Invariante als Test).

**Gate 4:** Auth-Konformität nachgewiesen; alle fail-closed-Negativtests grün.
**Rollback:** Additive Header/Doks entfernen; Kernauth unverändert.

---

## Phase 5 — Lifecycle & Capabilities (nur was die Spec verlangt)

- [ ] **5.1 `notifications/initialized`** akzeptieren, falls Pflicht (No-op-tauglich, aber
      spec-konform behandeln).
- [ ] **5.2 `ping`** beantworten, falls Pflicht.
- [ ] **5.3 Capabilities ehrlich.** In `initialize` nur ankündigen, was getestet ist. `tools`
      ggf. um neue Flags der Ziel-Revision erweitern — **kein** `resources`/`prompts`, solange
      nicht implementiert (bewusste Abgrenzung, analog Lexikon-Projektion).
- [ ] **5.4 Optionale Features bewerten** (strukturierte Tool-Outputs, Elicitation): pro Feature
      Nutzen vs. Aufwand; Ausschluss begründet dokumentieren.

**Gate 5:** Pflicht-Lifecycle grün; Capabilities-Deklaration == tatsächliches Verhalten.
**Rollback:** Einzelne Handler sind additiv; selektiv revertierbar.

---

## Phase 6 — Default-Version anheben (der eigentliche „Schalter")

**Ziel:** Erst jetzt wird die ausgehandelte **Default**-Version auf die Ziel-Revision gehoben.

- [ ] **6.1 ansV zuerst vorbereiten.** Falls die neue Default-Version vom ansV-Client einen
      `initialize`-Handshake oder neuen Transport erfordert: **zuerst ansV anpassen** (siehe
      Phase 7) und deployen, *dann* erst die Server-Default kippen.
- [ ] **6.2 Config-Flip.** `MCP_PROTOCOL_DEFAULT` auf Ziel-Revision (dank 2.3 ohne Code-Redeploy).
- [ ] **6.3 Konformanztest umstellen.** Baseline 1.1 auf die neue Version aktualisieren; alter
      Wert bleibt als „Alt-Client-Negotiation"-Test erhalten.
- [ ] **6.4 Provenance-Konsistenztest.** Test, der **bricht**, wenn
      Code-Default ≠ README-Badge ≠ ADR-008-Ziel.

**Gate 6:** Neuer Default ausgehandelt; ansV (Phase 7) bereits kompatibel deployt; Konsistenztest grün.
**Rollback:** `MCP_PROTOCOL_DEFAULT` zurück auf `2024-11-05` (sofortiger Config-Flip).

---

## Phase 7 — Konsument ansV nachziehen

**Ziel:** ansV nutzt den neuen Stand sauber; belegbare Kette bleibt intakt.

- [ ] **7.1 `McpClient` prüfen/erweitern.** Falls Ziel-Revision Handshake/Transport verlangt:
      `initialize` + Negotiation im Client ergänzen (`ansv-fedlex/src/client.rs`).
- [ ] **7.2 Mock & Live-Tests anpassen.** `e2e_belegkette.rs`-Mock und `live.rs` auf neuen
      Handshake/Transport erweitern; beide grün.
- [ ] **7.3 Staging-Lauf.** Voller Gutachten-Lauf (`examples/gutachten.rs` / Analyse-SSE) gegen
      die migrierte Reader-Instanz; `provenance` und Belegkette unverändert korrekt.
- [ ] **7.4 Reihenfolge wahren.** ansV-Deploy **vor** Server-Default-Flip (6.2), wenn der Flip
      breaking wäre; sonst entkoppelt möglich.

**Gate 7:** ansV-Unit+E2E+Live grün gegen migrierten Reader; Staging-Gutachten korrekt belegt.
**Rollback:** ansV-`MCP_FEDLEX_URL` auf alte Reader-Instanz/Version zeigen lassen.

---

## Phase 8 — Rollout, Docs, Release

- [ ] **8.1 Canary.** Migrierten Reader parallel/als Canary; Smoke + ansV-Live dagegen.
- [ ] **8.2 Live-Konformanz.** Wöchentlicher Daten-Konformanzlauf (jolux/akn/bridge) grün —
      Upgrade darf die Datenschicht nicht berühren, Beweis sichern.
- [ ] **8.3 Docs.** README-Abschnitt „An MCP-Client anbinden" (Protokollzeile), **MCP-Badge
      wieder einsetzen** mit nun korrekter Version, `CHANGELOG.md` (`v0.2.0`), ggf. `80_DEPLOY.md`.
- [ ] **8.4 ADR-008 schliessen.** Status `Accepted`, Akzeptanzkriterien abgehakt, Open-Items U-9
      / Block E auf erledigt.
- [ ] **8.5 Tag `v0.2.0`** — erst wenn 8.1–8.4 grün.

**Gate 8:** Prod grün (Smoke + ansV-Live + Konformanz), Docs konsistent, Tag gesetzt.
**Rollback:** Image auf gepinnten Vorher-Stand (1.5) + `MCP_PROTOCOL_DEFAULT=2024-11-05`.

---

## Phase 9 — Aufräumen (separat, nach Stabilisierungsfenster)

- [ ] **9.1 Alt-Transport ausmustern** (`/sse`+`/rpc`), **erst** wenn kein Client ihn mehr nutzt
      (ansV bestätigt umgestellt + Stabilisierungsfenster ohne Zugriffe auf Alt-Pfad).
- [ ] **9.2 Alt-Negotiation-Default entfernen**, wenn keine Alt-Clients mehr existieren.
- [ ] **9.3 Übergangs-Flags/Doppelpfade entfernen**; Tests aufräumen.

**Gate 9:** Zugriffslogs zeigen 0 Alt-Pfad-Nutzung über das Fenster.
**Rollback:** Aufräum-Commit revert (Alt-Pfad war bis hier erhalten).

---

## Risikoregister (kompakt)

| Risiko | Wahrscheinlichkeit | Wirkung | Gegenmassnahme |
| --- | --- | --- | --- |
| Transport-Deprecation bricht ansV | mittel | hoch | Doppelpfad (Phase 3), ansV zuerst (Phase 7), Default-Flip zuletzt (Phase 6) |
| ansV ohne `initialize` zerbricht an Pflicht-Handshake | mittel | hoch | Default-Version für fehlende Client-Version beibehalten (2.2) |
| Auth-Modell verlangt Pflicht-Metadaten | niedrig–mittel | mittel | additive Header/Discovery (Phase 4), fail-closed unangetastet |
| Falsche Provenance (Badge ≠ Verhalten) | niedrig | hoch (Vertrauensbruch) | Konsistenztest (6.4), Badge erst in 8.3 |
| Stiller Bruch der Datenschicht | niedrig | hoch | Konformanzlauf als Gate (8.2) |

## Abbruch-/Pausen-Regel

Wird ein Gate rot, **stoppt** die Migration in dieser Phase; der jeweilige Rollback wird
ausgeführt; Ursache wird in ADR-008 §A-2 (Delta-Matrix) oder hier als Befund ergänzt, bevor neu
angesetzt wird. Kein „Durchdrücken".

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

Der Reader ist **produktiv** (`https://mcp-fedlex.ch`) und hat **zwei echte Rust-Konsumenten
in Produktion**: die ansV-Plattform (`ansv-fedlex::McpClient`) und syllogismus-fedlex
(`McpFedlexClient`). **Beide** rufen direkt `POST /rpc` **ohne `initialize`** — ein unsauberes
Upgrade bricht nicht nur den Server, sondern die belegbare Gutachten-Kette beider Konsumenten.

**Verifizierter Ist-Zustand (Code-belegt):**

| Seite | Fakt | Beleg |
| --- | --- | --- |
| Server | Protokollversion hartcodiert `2024-11-05` | `crates/mcp-reader/src/transport.rs:217` |
| Server | Methoden: `initialize`, `tools/list`, `tools/call` | `transport.rs` (`match req.method`) |
| Server | Capabilities nur `{ "tools": {} }`; kein `ping`/`notifications`/`resources`/`prompts`/`logging` | `transport.rs` |
| Server | Transport: `GET /sse` (nennt POST-Endpoint), `POST /rpc` | `transport.rs` (`sse_handler`/`rpc_handler`) |
| Server | Auth: Bearer/JWT pro Request, fail-closed | ADR-002, `transport.rs` (`auth.verify`) |
| **Konsument 1** | ansV `McpClient` ruft **direkt** `POST {base}/rpc` mit `tools/list`/`tools/call` | `ansV/crates/ansv-fedlex/src/client.rs` |
| **Konsument 1** | ansV ruft **kein `initialize`** — handelt also heute **keine Version aus** | `client.rs` (nur `rpc("tools/list"/"tools/call")`) |
| **Konsument 2** | syllogismus-fedlex `McpFedlexClient` ruft **direkt** `POST {base}/rpc` mit `tools/call` (`read_article`) | `syllogismus-fedlex/src/mcp_client.rs` |
| **Konsument 2** | syllogismus-fedlex ruft ebenfalls **kein `initialize`** — gleiches additiv-kompatibles Muster wie ansV | `mcp_client.rs` (`fetch_norm` → `tools/call`) |
| Konsument | Live-Test gegen `https://mcp-fedlex.ch`, gated durch `MCP_FEDLEX_JWT` | `ansv-fedlex/tests/live.rs` |
| Konsument | E2E-Mock bildet `/rpc` mit `tools/list`+`tools/call` nach | `ansv-fedlex/tests/e2e_belegkette.rs` |
| Infra | Öffentlich via Ingress `mcp-fedlex.ch` | `k3-infra/.../mcp-fedlex/ingress.yaml` |
| Nicht-Konsument | `mcp-fedlex-web` ist die Zola-Doku-Site (statisches HTML), **kein** Protokoll-Client | `mcp-fedlex-web/` |


**Daraus folgt die zentrale Migrationsregel:**
> Es gibt **einen** Reader, und der wird auf die neue Spec gehoben — **kein Parallelbetrieb zweier
> Protokollstände**. Der alte Stand (`2024-11-05`, `/sse`+`/rpc`) ist **kein Erhaltungsziel**; ihn
> dauerhaft neben dem neuen am Leben zu halten macht das System nur komplizierter und fehleranfälliger,
> nicht besser. „Vorsichtig" heißt hier **nicht** Rückwärtskompatibilität, sondern: **koordinierter,
> einmaliger Umstieg** mit Gates, Tests und Rollback-Anker, damit wir beim Wechsel nichts
> **verschlechtern**. Weil **beide** Konsumenten (ansV, syllogismus-fedlex) heute **ohne `initialize`**
> und **direkt auf `/rpc`** arbeiten, werden sie **im selben Zug** auf den neuen Stand gezogen — die
> Reihenfolge (Clients bereit → Server-Umstieg) ist das Sicherheitsnetz, nicht ein zweiter Dauerpfad.
> Die `2024-11-05`-Baseline-Tests bleiben als **Vorher-Vergleich** erhalten (Regressions-Sichtbarkeit),
> nicht als Kompatibilitätsversprechen.


---

## Phase 0 — Vorbereitung & Faktenbasis (kein Code)

**Ziel:** Vollständige, verifizierte Grundlage. Keine Annahme bleibt ungeprüft.

- [x] **0.1 Spec-Quelle fixieren.** Offizielle Revisionen aus `modelcontextprotocol/modelcontextprotocol`
      (first-hand via `raw.githubusercontent.com`, abgerufen 2026-06-20): stabile Reihe
      `2024-11-05` → `2025-03-26` → `2025-06-18` → **`2025-11-25`** (plus `draft`). **Ziel-Revision:
      `2025-11-25`** (höchste stabile; `2025-06-18` nur Wegpunkt, `draft` gemieden). Quellen pro
      Revision: `changelog.mdx`, `basic/transports.mdx`, `basic/lifecycle.mdx`. *(2026-06-20)*
- [x] **0.2 Delta-Matrix erstellt.** Pro Revision ab `2024-11-05` bis Ziel je Zeile mit Klasse
      `NEU | GEÄNDERT | DEPRECATED | BREAKING` — Single Source of Truth in **ADR-008 §A-2/A-3**.
      *(2026-06-20)*
- [x] **0.3 Betroffenheits-Mapping.** Jede Delta-Zeile gegen die Ist-Tabelle (§0) klassifiziert
      (`betrifft uns | optional | irrelevant`), siehe ADR-008 §A-2/A-3 + Zusammenfassung. Kern:
      Transport (Streamable HTTP, `MCP-Protocol-Version`-Header, 400/403), Lifecycle (`ping`,
      `notifications/initialized` SHOULD→MUST), Auth (Resource-Server additiv), JSON Schema 2020-12.
      *(2026-06-20)*
- [x] **0.4 Konsumenten-Inventar abgeschlossen.** Verifiziert per grep über **alle** Repos
      (`mcp-fedlex.ch` / `McpClient` / `MCP_FEDLEX_URL`). **Zwei** Produktions-Rust-Konsumenten,
      beide direkt `/rpc` **ohne `initialize`**: (1) ansV `ansv-fedlex::McpClient`, (2)
      syllogismus-fedlex `McpFedlexClient` (`mcp_client.rs`, `tools/call read_article`). Kein
      weiterer Protokoll-Client; `mcp-fedlex-web` ist statische Doku-Site (siehe Ist-Tabelle §0).
      *(2026-06-20)*
- [x] **0.5 Abbruchkriterien definiert.** Die Migration **pausiert sofort** (mit Rollback der
      laufenden Phase), wenn **eines** zutrifft:
      1. Baseline-Konformanz (1.1/1.2) oder Lexicon-Projektion rot;
      2. ansV-E2E (`e2e_belegkette.rs`) **oder** syllogismus-fedlex-Normbezug bricht;
      3. ein fail-closed-Negativtest (Auth/ADR-002) wird grün-durchlässig;
      4. Quota-Kette (ADR-007) wird umgangen oder Daten-Konformanzlauf (jolux/akn/bridge) rot;
      5. Provenance-Konsistenz (ausgehandelte Version ≠ Badge ≠ ADR-Ziel) bricht.
      *(2026-06-20)*

**Gate 0:** ✅ ADR-008 §A-1…A-4 ausgefüllt, Delta-Matrix reviewt, Ziel-Revision (`2025-11-25`)
schriftlich fixiert, Konsumenten-Inventar (2 Clients) + Abbruchkriterien verifiziert. *(2026-06-20)*
**Rollback:** entfällt (reine Doku).

---

## Phase 1 — Sicherheitsnetz VOR jeder Code-Änderung

**Ziel:** Regressionen werden mechanisch sichtbar, bevor wir irgendetwas anfassen.

- [x] **1.1 Baseline-Konformanztest (alt).** Test, der den **heutigen** `initialize`-Handshake
      festschreibt (`protocolVersion == "2024-11-05"`, `capabilities.tools` vorhanden,
      `serverInfo.name`). **Grün** in `crates/mcp-reader/tests/protocol_baseline.rs`
      (`initialize_handshake_is_frozen_at_2024_11_05`). *(2026-06-20)*
- [x] **1.2 Methoden-Snapshot.** Test, der die exakte Antwortform von `tools/list`
      und eines `tools/call` (inkl. `provenance`-Block) fixiert, plus fail-closed-Invarianten
      (fehlendes/ungültiges Credential → −32001, unbekannte Methode → −32601). **Grün** in
      `protocol_baseline.rs` (6 Tests). *(2026-06-20)*
      > **Aktualisiert (2026-06-20, Phase 7.2a):** Die Eintragsform war ursprünglich auf
      > `{name, schema}` (**nicht** `inputSchema`) eingefroren. Nach dem additiven Wire-Delta
      > prüft 1.2 nun, dass **beide** Schlüssel präsent sind (`inputSchema` **und** `schema`)
      > **und denselben Wert tragen**. Der Legacy-Schlüssel fällt erst in Phase 9.

- [x] **1.3 Konsumenten-Vertragstests grün stellen — vollständig (Live-Lauf 2026-06-20).**
      Offline-Teil **erledigt & re-verifiziert**:
      - **Konsument 1 (ansV):** `cargo test -p ansv-fedlex` grün — **22 Unit + 2 E2E-Mock**
        (`e2e_belegkette.rs`: `frage_ohne_eli_endet_verifiziert`, `geratener_eli_bleibt_unverifiziert`);
        `live.rs` (2 Tests) zuvor erwartungsgemäss `ignored` (braucht `MCP_FEDLEX_JWT` + Netz).
      - **Konsument 2 (syllogismus-fedlex):** `cargo test` grün — **10 Unit**, 0 ignored.
      - **Reader-Baseline gegengeprüft:** `cargo test -p mcp-reader` → **135 Unit + 6 Baseline
        (`protocol_baseline.rs`) + 1 Lexicon** grün; `quota_integration` `ignored` (braucht Docker).
      **Referenz-Live-Lauf archiviert (2026-06-20):** Mit frisch gemintetem Navigator-JWT
      (`tests-eval/lib/mint_jwt.py`, HS256 aus `mcp-reader-jwt`) lief
      `MCP_FEDLEX_JWT=… cargo test -p ansv-fedlex --test live -- --ignored` → **2 passed, 0 failed**
      gegen `https://mcp-fedlex.ch` (`tools_list_liefert_katalog`, `read_article_mit_evidence_log`
      inkl. `provenance.eli` + `valid_as_of`). Damit ist der Konsumentenvertrag gegen die laufende
      Prod-Instanz belegt — Vorher-Stand für den Upgrade-Vergleich.

- [x] **1.4 Smoke-Baseline — grün dokumentiert (2026-06-20).** Direkt gegen `https://mcp-fedlex.ch`
      mit Validator-JWT verifiziert:
      - **Health:** `/livez` 200, `/readyz` 200 (`/startupz` nur clusterintern, via Ingress 404 —
        erwartbar, kein Smoke-Kriterium).
      - **fail-closed:** `initialize` **ohne** Token → `-32001 "missing credential"` (ADR-002 bestätigt).
      - **initialize:** `protocolVersion = 2024-11-05`, `serverInfo = {mcp-fedlex-reader, 0.1.0}`,
        `capabilities = {tools}` — exakt die ADR-008-Baseline.
      - **tools/list:** 22 Tools; Eintragsform trägt **`schema`** (nicht `inputSchema`) → **Befund #1
        live auf Prod bestätigt** (siehe ADR-008 §B-5 / Risikoregister).
      - **tools/call `read_article`** (`eli=eli/cc/24/233_245_233`, `eid=art_1`) → `{data, provenance}`,
        `provenance != null`. Provenance-Pfad end-to-end belegt.
- [x] **1.5 Image-Pin für Rollback — erledigt (2026-06-20).** Befund präzisiert: Das Deployment ist
      **ArgoCD-verwaltet** (`argocd.argoproj.io/tracking-id: mcp-fedlex:apps/Deployment:mcp-fedlex/mcp-reader`),
      Namespace `mcp-fedlex` auf Cluster-Context `default`. Ein `kubectl patch` würde driften und vom
      Reconcile überschrieben → der Pin **muss in die GitOps-Quelle** (kein imperativer Patch).
      Durchgeführt:
      1. **Laufenden Digest gelesen** (beide Replicas identisch, Selektor
         `app.kubernetes.io/name=mcp-reader`):
         `registry.mindful-server.com/mindful-bio/mcp-fedlex@sha256:d97e3811239cc71b3eb7e5d7003f8965ad99fdc3c19f240a93e71fc85de911d3`
         (via `kubectl -n mcp-fedlex get pods -l app.kubernetes.io/name=mcp-reader -o jsonpath='{..imageID}'`;
         Deployment-Tag war `:latest`, `imagePullPolicy: Always`, `replicas: 2`, `maxUnavailable: 0`).
      2. **`reader.yaml` gepinnt** auf genau diesen `@sha256:…` + `imagePullPolicy: IfNotPresent`
         (Digest ist selbst-immutabel). **Gegenprobe (2026-06-20):** beide laufenden Pods
         (`6599b7b5b6-jvwfk`, `-rvvrf`) tragen exakt diesen Digest → Pin == Prod-Ist.
         Damit ist Gate-6/8-Rollback ein exakter Tausch.
      **GitOps-Übernahme — abgeschlossen (2026-06-20):** Commit `f9abdd3` in `k3-infra` (main)
      gepusht; ArgoCD-App `mcp-fedlex` nach Refresh auf `revision=f9abdd3`, **Synced/Healthy**;
      Deployment trägt jetzt `@sha256:…911d3` + `IfNotPresent`. **Ehrliche Korrektur einer
      Vorab-Annahme:** Der Sync war **kein** No-Op — der geänderte Pod-Template-String
      (`:latest`→`@sha256`, `Always`→`IfNotPresent`) erzeugt einen neuen Template-Hash und damit
      ein **rollendes Re-Deploy** (neues ReplicaSet `6645958bb4`). Dank `maxUnavailable: 0` lief das
      **ohne Downtime**, und da der Digest bit-identisch ist, läuft **exakt derselbe Code** weiter.
      Post-Sync-Smoke grün: `/livez` 200, `initialize` → `2024-11-05`. Der Pin wirkt fortan als
      Rollback-Anker.
      *(Reine Doku-/Test-Phasen 0–2 sind davon unberührt; der Pin ist Vorbedingung für Gate 6.)*

**Gate 1:** ✅ **erreicht (2026-06-20).** Alle Baseline-Tests grün und als „Vorher-Stand" archiviert
(1.1/1.2 in `protocol_baseline.rs`); Konsumentenvertrag offline **und** live grün (1.3); Smoke gegen
Prod grün (1.4); Rollback-Digest gepinnt (1.5, GitOps-Commit/Sync als einziger offener Prod-Schritt).
**Rollback:** entfällt (nur additive Tests).

---

## Phase 2 — Versions-Negotiation (additiv, noch kein Verhalten geändert)

**Ziel:** Der Server *kann* mehrere Versionen, verhält sich aber für Alt-Clients identisch.

- [x] **2.1 `SUPPORTED_PROTOCOL_VERSIONS`** als sortierte Konstante eingeführt (heute nur
      `["2024-11-05"]`, aufsteigend; wächst erst, wenn eine Revision implementiert+getestet ist).
      Single Source of Truth in `crates/mcp-reader/src/protocol.rs`, plus `highest_supported`,
      `is_supported`, `negotiate`, `default_protocol_version`. *(2026-06-20)*
- [x] **2.2 `initialize`-Negotiation.** In `transport.rs` verdrahtet (`McpService::handle`):
      - bekannt & gemeinsam → diese aushandeln;
      - **fehlt** (heutiger ansV-Fall!) → Default-Version (weiterhin `2024-11-05`);
      - unbekannt/zu neu → `highest_supported()` (spec-konform, kein harter Fehler).
      *(2026-06-20)*
- [x] **2.3 Default-Version als Config.** `MCP_PROTOCOL_DEFAULT` via `default_protocol_version()`
      (gesetzt+unterstützt → Override; sonst fail-safe Kompilier-Default). `McpService::new` löst
      aus der Umgebung auf; `with_protocol_default(...)` für deterministische Tests. Damit ist der
      spätere Sprung ein **Config-Flip ohne Redeploy-Code**. *(2026-06-20)*
- [x] **2.4 Tests.** Negotiation-Matrix grün: 6 Unit-Tests im `protocol`-Modul (sortiert/nichtleer,
      Default-ist-supported, highest=letztes, fehlend→Default, bekannt→echo, unbekannt→highest) +
      4 Transport-Tests (`initialize_without_client_version_uses_default`,
      `…_echoes_supported_client_version`, `…_with_unknown_version_offers_highest_supported`,
      `…_default_is_config_driven`). **Baseline 1.1 unverändert grün** (Default unangetastet).
      Gesamt: `cargo test -p mcp-reader` → **135 unit + 6 baseline + 1 lexicon** grün. *(2026-06-20)*

**Gate 2:** Negotiation-Tests grün; Baseline 1.1–1.4 weiterhin grün; ansV-E2E unverändert grün.
**Rollback:** Feature ist additiv; Default unverändert ⇒ Risiko ~0. Notfalls Commit revert.

---

## Phase 3 — Transport-Umstieg auf Streamable HTTP (der riskanteste Teil)

**Ziel:** Die Ziel-Revision deprecatet den HTTP+SSE-Weg (→ „Streamable HTTP"). Der neue Transport
wird umgesetzt und der alte (`/sse`+`/rpc`) im Zuge des Umstiegs **ersetzt**, **nicht** dauerhaft
parallel gepflegt. Ein kurzer Übergang, in dem der alte Pfad noch antwortet, ist nur zulässig,
**bis beide Clients (Phase 7) umgestellt sind** — danach wird er entfernt (Phase 9). Es bleibt also
**ein** Zielzustand, kein Dauer-Doppelbetrieb.


- [x] **3.1 Entscheidung aus Delta-Matrix — JA, betrifft uns (2026-06-20).** Laut ADR-008 §A-2
      (Revision `2025-03-26`, Zeile 2) ist HTTP+SSE → **Streamable HTTP** als DEPRECATED/**BREAKING**
      klassifiziert; dazu `MCP-Protocol-Version`-Header-Pflicht (`2025-06-18` #8), **400** bei
      unbekannter Version und **403** bei ungültigem `Origin` (`2025-11-25` #12). Phase 3 wird also
      **nicht** übersprungen — Doppelpfad (3.2) ist zwingend.
      > **Zu klärende Reconciliation (Phase 3↔6):** Es gibt **zwei** verschiedene Fallback-Werte:
      > (a) fehlende **`initialize`-`protocolVersion`** → Negotiation-Default `2024-11-05`
      > (Runbook 2.2/2.3); (b) fehlender **HTTP-`MCP-Protocol-Version`-Header** → Spec-SHOULD
      > `2025-03-26` (ADR-008 §Header-Regel). Beide Pfade müssen in 3.4 explizit getestet und ihr
      > Zusammenspiel beim Default-Flip (6.2) bewusst gesetzt werden — sonst droht ein stiller
      > Versions-Mismatch zwischen Handshake- und Header-Ebene.
      > **Teilschritt ERLEDIGT (2026-06-20):** Die **Header-Ebene** ist als reine, noch **nicht
      > verdrahtete** Klassifikation in `protocol.rs` vorbereitet: `classify_protocol_header()` →
      > `ProtocolHeaderOutcome::{Absent, Supported, Unsupported}`. Bewusst getrennt von
      > [`negotiate`] (Handshake-Ebene), damit der unterschiedliche Fallback explizit im Typ steht:
      > **fehlender/leerer Header → `Absent` (kein 400)** — schützt die header-losen Alt-Clients
      > (ansV, syllogismus-fedlex); gesetzte unbekannte Version → `Unsupported` (= späterer HTTP
      > **400**, erst mit Streamable HTTP live). 5 Unit-Tests fixieren das Verhalten;
      > `cargo test -p mcp-reader` → **141 unit + 6 baseline + 1 lexicon** grün. Die Verdrahtung in
      > den Request-Pfad folgt erst mit dem Streamable-HTTP-Endpoint (3.2 Rest) + 3.4.

- [ ] **3.2 Neuen Transport implementieren (Streamable HTTP).** Neuer Endpoint/Modus. Der alte
      `/rpc`-Pfad bleibt **nur übergangsweise** erreichbar, **bis beide Clients (Phase 7) umgestellt
      sind** — er ist kein Dauerzustand, sondern wird in Phase 9 entfernt. Ziel ist der neue
      Transport als **einziger** Pfad.

      > **⚠ Struktureller Vorab-Umbau — ERLEDIGT (2026-06-20):** `rpc_handler` gab bisher
      > `Json<JsonRpcResponse>` zurück (immer HTTP 200 mit JSON-Body, selbst bei Auth-/Parse-
      > Fehlern) und konnte **keine** der neuen Status-Anforderungen ausdrücken (202/204 ohne
      > Body für Notifications 5.1/B-4; **400** unbekannte Protokollversion; **403** ungültiger
      > `Origin`; **401**). Umgestellt auf `axum::response::Response` via `.into_response()` auf
      > **beiden** Pfaden (Parse-Fehler + Normalfall) — **verhaltensneutral**: Alt-Clients (ansV,
      > syllogismus-fedlex) sehen bit-identisch weiter 200+JSON. Abgesichert durch neuen Test
      > `rpc_handler_keeps_200_json_for_all_paths` (HTTP-200 + `content-type: application/json` +
      > `jsonrpc`-Marker für Parse- und Auth-Fehlerpfad). **Baseline 1.2 unverändert grün**;
      > Gesamtlauf `cargo test -p mcp-reader` → **136 unit + 6 baseline + 1 lexicon** grün.
      > Damit ist die Signatur bereit für die Status-Codes/No-Body-Pfade der nächsten Schritte,
      > **ohne** erneuten Signatur-Umbau.
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
      > **⚠ Verifiziert nötig (2026-06-20):** Heute fehlt jegliche Notification-Behandlung.
      > `JsonRpcRequest.id` ist `#[serde(default)] Value` → eine Notification (ohne `id`) wird zu
      > `id = Value::Null`, und `McpService::handle` antwortet auf **jeden** Pfad — eine unbekannte
      > Methode wie `notifications/initialized` ergäbe fälschlich `-32601`. Korrektur (zweistufig):
      > (a) `id` als `Option<Value>` modellieren, um „Feld fehlt" (Notification) von „`id: null`"
      > (Request) zu unterscheiden; (b) bei Notifications **keinen** Response-Body erzeugen; (c)
      > der `/rpc`-HTTP-Handler muss diesen No-Body-Fall mit **HTTP 202/204 ohne JSON** umsetzen
      > (siehe 3.x), statt eine leere JSON-RPC-Response zu serialisieren.
      > **Teilschritt (a) ERLEDIGT (2026-06-20):** `JsonRpcRequest.id` ist von
      > `#[serde(default)] Value` auf **`Option<Option<Value>>`** mit eigenem
      > `deserialize_with`-Adapter (`deserialize_present_id`) umgestellt. Das **doppelte
      > `Option` ist bedeutungstragend**, weil serde ein anwesendes `null` sonst auf `None`
      > kollabieren würde: *Feld fehlt* → `None` (**Notification**), *`id: null`* → `Some(None)`
      > (Request, Null-Id), *`id: 7`* → `Some(Some(7))`. Die Klassifikation liegt in
      > `JsonRpcRequest::is_notification()`; das **Antwortverhalten bleibt strikt verhaltensneutral**:
      > der neue `response_id()`-Helfer bildet beide Null-Fälle weiterhin auf `Value::Null` ab, und
      > alle bestehenden Call-Sites (`initialize`/`tools/list`/`tools/call`/Fehlerpfade) nutzen ihn,
      > sodass Alt-Clients (ansV, syllogismus-fedlex) bit-identisch dieselbe Antwort sehen. Drei
      > Unit-Tests fixieren die Drei-Wege-Unterscheidung über die Wire-Grenze
      > (`notification_is_classified_and_maps_to_null_response_id`,
      > `explicit_null_id_is_a_request_not_a_notification`,
      > `missing_id_field_deserializes_as_notification`). `cargo test -p mcp-reader` → **144 unit +
      > 6 baseline + 1 lexicon** grün. **Offen bleiben (b)** kein Response-Body bei Notifications und
      > **(c)** der HTTP-202/204-No-Body-Pfad — beide erst mit dem Streamable-HTTP-Endpoint (3.2 Rest).
- [ ] **5.2 `ping`** beantworten, falls Pflicht (echte Request/Response **mit** `id` —
      nur ein zusätzlicher Match-Arm, von 5.1 unberührt).

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

**Gate 6:** Neuer Default ausgehandelt; ansV (Phase 7) bereits kompatibel deployt; Konsistenztest grün;
**Vorbedingung: Image-Pin aus 1.5 erledigt** (sonst kein belastbarer Image-Rollback).
**Rollback:** `MCP_PROTOCOL_DEFAULT` zurück auf `2024-11-05` (sofortiger Config-Flip).

---

## Phase 7 — Konsumenten nachziehen (ansV + syllogismus-fedlex)

**Ziel:** ansV nutzt den neuen Stand sauber; belegbare Kette bleibt intakt.

> **Hinweis (§0.4):** syllogismus-fedlex (`McpFedlexClient`) ist der **zweite** Alt-Client und
> muss bei einem Handshake-/Transport-Zwang **analog** nachgezogen werden (eigener `initialize`-/
> Streamable-HTTP-Pfad in `mcp_client.rs`), bevor Phase 9 den Alt-Pfad ausmustert.


- [ ] **7.1 `McpClient` prüfen/erweitern.** Falls Ziel-Revision Handshake/Transport verlangt:
      `initialize` + Negotiation im Client ergänzen (`ansv-fedlex/src/client.rs`).
- [ ] **7.2 Mock & Live-Tests anpassen.** `e2e_belegkette.rs`-Mock und `live.rs` auf neuen
      Handshake/Transport erweitern; beide grün.
- [x] **7.2a `inputSchema`-Feld additiv umgesetzt — erledigt (2026-06-20).**
      **Server:** `registry.rs::list_tools` emittiert pro Tool jetzt **beide** Schlüssel mit
      demselben Wert — `inputSchema` (MCP-Standard) **und** `schema` (Legacy) — als additiven
      Doppel-Output. Baseline-Test 1.2 (`protocol_baseline.rs`) prüft Präsenz beider Felder und
      ihre Wertgleichheit.
      **Client:** `tool_def_from_mcp` (`ansv-fedlex/src/llm.rs`) liest nun **`inputSchema` bevorzugt,
      `schema` als Fallback** (forward- & backward-kompatibel); zwei Unit-Tests sichern beide Pfade.
      Der E2E-Mock (`e2e_belegkette.rs`) spiegelt die echte Doppel-Form. Damit ist der Alt-Feldname
      `schema` erst in **Phase 9** gefahrlos entfernbar. (syllogismus-fedlex unkritisch: ruft nie
      `tools/list`.)

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
| Kein exakter Image-Rollback: Deployment auf `:latest` (`imagePullPolicy: Always`) | hoch (heute real) | hoch | Entschärfbar: CI pusht bereits `:<sha>`/`:vX.Y.Z` (Tags liegen in der Registry); vor Phase 6 Deployment auf Digest/SemVer pinnen (1.5) |
| Feldname `schema`→`inputSchema` bricht ansV (liest `schema` aktiv, `llm.rs`) | hoch (bei hartem Schnitt) | hoch | Additiver Doppel-Output beider Felder; ansV-Fallback-Update (7.2a) vor Phase-9-Bereinigung |

## Abbruch-/Pausen-Regel

Wird ein Gate rot, **stoppt** die Migration in dieser Phase; der jeweilige Rollback wird
ausgeführt; Ursache wird in ADR-008 §A-2 (Delta-Matrix) oder hier als Befund ergänzt, bevor neu
angesetzt wird. Kein „Durchdrücken".

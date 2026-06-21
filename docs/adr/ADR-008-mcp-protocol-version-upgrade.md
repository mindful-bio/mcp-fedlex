# ADR-008: MCP-Protokollversion — Upgrade & Versions-Negotiation

- **Status:** Proposed — §A (Spec-Recherche) erledigt; §B/§C noch nicht umgesetzt
- **Datum:** 2026-06-20 (§A aktualisiert 2026-06-20)
- **Kontext-Artefakt:** `crates/mcp-reader/src/transport.rs` (Handshake), `likec4/`
- **Betrifft:** `mcp-fedlex` (Reader), alle MCP-Clients (Agenten, Claude Desktop, ansV,
  syllogismus-fedlex) — die beiden Rust-Konsumenten ansV (`ansv-fedlex::McpClient`) und
  syllogismus-fedlex (`McpFedlexClient`) rufen heute beide `/rpc` ohne `initialize`
  (Konsumenten-Inventar: Runbook 55, Phase 0.4)
- **Ziel-Revision:** **`2025-11-25`** (höchste stabile MCP-Spec-Revision; `2025-06-18` ist als Ziel ausgeschlossen)
- **Folge-Release:** Ziel `v0.2.0` (siehe `CHANGELOG.md`)

- **Operatives Runbook:** [`docs/55_MIGRATION_mcp_protocol_upgrade.md`](../55_MIGRATION_mcp_protocol_upgrade.md)
  — Phasen 0–9 mit Gates & Rollback je Schritt (extrem-vorsichtige Ausführung)


## Kontext

Der Reader handelt im `initialize`-Handshake heute die Protokollversion
**`2024-11-05`** aus — als **eine hartcodierte Konstante** in
`transport.rs:217`. Diese Revision ist veraltet; das MCP hat seither mehrere
Spec-Revisionen erhalten. Eine veraltete ausgehandelte Version bedeutet:

1. **Falsche Provenance nach aussen.** README-Badge und Handshake behaupten
   genau die Konformität, die ausgehandelt wird — nicht mehr und nicht weniger.
   Ein zu niedriger Wert verschenkt neuere Client-Fähigkeiten; ein blind zu hoch
   gesetzter Wert *lügt* über Konformität, die nicht getestet ist. Beides ist im
   Sinne des Provenance-Prinzips (ADR-004) unzulässig.
2. **Verpasste Transport-/Sicherheitsfeatures.** Neuere Revisionen führten u. a.
   einen vereinheitlichten HTTP-Transport und ein klareres Auth-Modell ein,
   die unsere `/sse`+`/rpc`-Kante direkt betreffen.

Wichtig für die Substanz dieses ADR: Der Rechercheschritt (§A) ist **erledigt**
und die Deltas sind **aus erster Hand gegen die offizielle Spec verifiziert**
(siehe Delta-Matrix in §A). **Ziel-Revision ist `2025-11-25`.** Erst auf dieser
Grundlage folgt die Umsetzung (§B/§C). Kein String wurde „blind" hochgesetzt.


## Heutiger Ist-Zustand (verifiziert im Code)

Bewusst kleiner, gut abgrenzbarer Methoden-Footprint:

| Bereich | Implementiert | Quelle |
|---|---|---|
| `initialize` | ja — gibt `protocolVersion`, `serverInfo`, `capabilities` | `transport.rs` |
| `tools/list` | ja — RBAC-gefiltert | `transport.rs` |
| `tools/call` | ja — Quota + Provenance-Gate | `transport.rs` |
| Capabilities | nur `{ "tools": {} }` | `transport.rs` |
| Transport | `GET /sse` (nennt POST-Endpoint), `POST /rpc` | `transport.rs` |
| `ping` | **nein** | — |
| `notifications/initialized` u. a. | **nein** | — |
| `resources/*`, `prompts/*`, `logging/*`, `completion/*` | **nein** (bewusst) | — |
| Auth | Bearer/JWT pro Anfrage, fail-closed | ADR-002 |

Daraus folgt: Der Aufwand liegt **nicht** in der Methodenbreite, sondern in
Handshake-Semantik, Transport-Konvention und Auth-Mapping.

## Entscheidung

Statt einer einzelnen hartcodierten Konstante führen wir eine **Versions-
Negotiation nach Spec-Vorgabe** ein und richten die Implementierung an der
**aktuell höchsten offiziellen Revision** aus, die wir nachweislich erfüllen.

### Leitprinzipien
- **Negotiation statt Konstante.** Der Server kennt eine sortierte Liste
  `SUPPORTED_PROTOCOL_VERSIONS`. Im `initialize` wählt er die höchste, die er
  *und* der Client unterstützen; nennt der Client keine, gilt die neueste eigene.
- **Ehrliche Provenance.** Ausgehandelte Version, README-Badge und ein
  Konformanztest müssen denselben Wert tragen. Single Source of Truth: eine
  Konstante im Code, gespiegelt durch Test + Badge.
- **Kein Feature ohne Test.** Jede neu beanspruchte Capability bekommt einen
  Konformanztest, sonst wird sie nicht angekündigt.

## §A — Spec-Recherche (Gate, MUSS zuerst) — ✅ ERLEDIGT (2026-06-20)

Ohne diesen Schritt geht keine Code-Änderung live.

- [x] **A-1 Offizielle Quelle gezogen.** Aus dem offiziellen Repo
      `modelcontextprotocol/modelcontextprotocol` (`docs/specification/`) sind die
      **stabilen Revisionen** verifiziert: `2024-11-05`, `2025-03-26`,
      `2025-06-18`, **`2025-11-25`** (plus `draft`). Quelle pro Revision:
      `changelog.mdx`, `basic/transports.mdx`, `basic/lifecycle.mdx` (first-hand
      via `raw.githubusercontent.com`, abgerufen 2026-06-20).
- [x] **A-2 Delta-Matrix erstellt** (siehe unten) — pro Revision ab `2024-11-05`
      mit Klassifikation `NEU | GEÄNDERT | DEPRECATED | BREAKING`.
- [x] **A-3 Betroffenheit markiert** — jede Zeile auf den §Ist-Zustand abgebildet:
      `betrifft uns | optional | irrelevant`.
- [x] **A-4 Ziel-Revision festgelegt: `2025-11-25`** (höchste stabile Revision),
      in `CHANGELOG.md` (`Unreleased` → `v0.2.0`) vorgemerkt.

### A-4 Ziel-Revision — Entscheidung

> **Ziel ist `2025-11-25`.** Es wird **direkt** auf die höchste stabile Revision
> migriert. Die Zwischenrevision **`2025-06-18` ist als Ziel ausgeschlossen** —
> sie erscheint hier nur als Wegpunkt in der Changelog-Kette, niemals als
> auszuhandelnder Wert. Ebenso wird `draft` bewusst gemieden (kein stabiler
> Provenance-Anker). Die ehrliche Provenance-Regel (ADR-004) bleibt: ausgehandelte
> Version == README-Badge == ADR-Ziel == Konformanztest == **`2025-11-25`**.

### A-2/A-3 Delta-Matrix (verifiziert, gegen unseren Footprint gemappt)

**Lesehilfe Betroffenheit:** _betrifft uns_ = erfordert Code/Doku-Arbeit;
_optional_ = Feature, das wir bewusst übernehmen oder begründet ausschliessen;
_irrelevant_ = berührt unseren Footprint (3 Methoden, `tools`-Capability, HTTP)
nicht.

#### Revision `2025-03-26` (gegen `2024-11-05`)

| # | Änderung | Klasse | Betroffenheit |
|---|---|---|---|
| 1 | OAuth-2.1-Authorization-Framework | NEU | **betrifft uns** (Auth-Mapping §B-3; unser Bearer/JWT bleibt fail-closed, ADR-002) |
| 2 | **Streamable HTTP** ersetzt HTTP+SSE | DEPRECATED/**BREAKING** | **betrifft uns** (Transport §B-2; Kernstück der Migration) |
| 3 | JSON-RPC-Batching | NEU | irrelevant (in `2025-06-18` wieder entfernt; nie implementiert) |
| 4 | Tool-Annotations (read-only/destructive …) | NEU | optional (unsere Tools sind read-only → ehrlich annotierbar) |
| 5 | `message`-Feld in `ProgressNotification` | NEU | irrelevant (keine Progress-Notifications) |
| 6 | Audio-Content-Type | NEU | irrelevant |
| 7 | `completions`-Capability | NEU | irrelevant (kein Completion-Support) |

#### Revision `2025-06-18` (gegen `2025-03-26`) — nur Wegpunkt, **kein Ziel**

| # | Änderung | Klasse | Betroffenheit |
|---|---|---|---|
| 1 | JSON-RPC-Batching **entfernt** | GEÄNDERT | irrelevant (nie implementiert) |
| 2 | Strukturierte Tool-Outputs (`structuredContent`) | NEU | optional (§B-6 abwägen; unser `provenance`-Block bleibt führend) |
| 3 | Server als **OAuth Resource Server** + Protected-Resource-Metadata | NEU | **betrifft uns** (Auth §B-3, additiv) |
| 4 | Resource Indicators (RFC 8707) als **Client**-Pflicht | NEU | irrelevant (client-seitig; wir sind Server) |
| 5 | Security-Considerations + Best-Practices-Seite | GEÄNDERT | **betrifft uns** (Doku/SECURITY.md abgleichen) |
| 6 | **Elicitation** | NEU | optional (**bewusst ausschliessen** — read-only Reader, analog Lexikon-Projektion) |
| 7 | Resource-Links in Tool-Ergebnissen | NEU | optional (begründet ausschliessen, kein `resources`) |
| 8 | **`MCP-Protocol-Version`-Header** auf Folge-Requests (HTTP) Pflicht | NEU/**BREAKING** | **betrifft uns** (Transport §B-2; siehe Header-Regel unten) |
| 9 | Lifecycle-Operation **SHOULD→MUST** | GEÄNDERT | **betrifft uns** (Lifecycle §B-4) |
| 10 | `_meta`-Feld an weiteren Typen | NEU | optional |
| 11 | `context`-Feld in `CompletionRequest` | NEU | irrelevant (kein Completion) |
| 12 | `title`-Feld (Anzeigename neben `name`) | NEU | optional (Tool-Titel; nutzwertig, geringer Aufwand) |

#### Revision `2025-11-25` (gegen `2025-06-18`) — **ZIEL**

| # | Änderung | Klasse | Betroffenheit |
|---|---|---|---|
| 1 | OIDC-Discovery 1.0 für Authorization-Server | NEU | optional (Auth; nur falls wir AS-Discovery anbieten) |
| 2 | **Icons** als Metadaten für Tools/Resources/Prompts | NEU | optional |
| 3 | Inkrementeller Scope-Consent via `WWW-Authenticate` | NEU | optional (Auth) |
| 4 | Guidance zu Tool-Namen | GEÄNDERT | optional (unsere Namen prüfen) |
| 5 | `ElicitResult`/`EnumSchema` standardnäher | GEÄNDERT | irrelevant (keine Elicitation) |
| 6 | URL-Mode-Elicitation | NEU | irrelevant (keine Elicitation) |
| 7 | Tool-Calling in Sampling (`tools`/`toolChoice`) | NEU | irrelevant (Client-Capability) |
| 8 | OAuth Client-ID-Metadata-Documents | NEU | optional (client-/Auth-seitig) |
| 9 | **Tasks** (experimentell, durable requests) | NEU | optional (**bewusst ausschliessen** — kein Bedarf) |
| 10 | stderr-Logging-Klarstellung (stdio) | GEÄNDERT | irrelevant (wir nutzen HTTP, nicht stdio) |
| 11 | `Implementation.description` optional | NEU | optional (in `serverInfo` ergänzbar) |
| 12 | **HTTP 403** bei ungültigem `Origin`-Header (Streamable HTTP) | GEÄNDERT | **betrifft uns** (Transport-Security §B-2) |
| 13 | Input-Validation-Fehler als **Tool-Execution-Error** statt Protocol-Error | GEÄNDERT | **erledigt (2026-06-21)** — beide `tools/call`-Validierungspfade (fehlendes `name`, ungültiges `as_of`) liefern jetzt in-band graceful `{ error, hint }` im `result`, formgleich zu Dispatch (ADR-006) und Quota-Pfad; `INVALID_PARAMS` (`-32602`) wurde aus `transport.rs` entfernt. Tests: `missing_tool_name_is_in_band_tool_error_not_protocol_error`, `invalid_as_of_is_in_band_tool_error_not_protocol_error` |

| 14 | Polling-SSE-Streams / SEP-1699-Klarstellungen | GEÄNDERT | optional |
| 15 | `WWW-Authenticate` optional + `.well-known`-Fallback (RFC 9728) | GEÄNDERT | optional (Auth) |
| 16 | Default-Werte in Elicitation-Schemas | NEU | irrelevant |
| 17 | **JSON Schema 2020-12** als Default-Dialekt | GEÄNDERT | **betrifft uns** (Tool-`inputSchema`-Dialekt prüfen) |
| 18 | Request-Payloads von RPC-Method-Defs entkoppelt | GEÄNDERT | irrelevant (Schema-intern, kein Wire-Impact für uns) |

### Zusammenfassung Betroffenheit (was §B konkret anfassen muss)

1. **Transport (§B-2).** HTTP+SSE → **Streamable HTTP** (single MCP-Endpoint mit
   POST+GET), **`MCP-Protocol-Version`-Header** auf Folge-Requests, **400** bei
   ungültiger/unbekannter Protokollversion, **403** bei ungültigem `Origin`.

   **Zielzustand ist der saubere `2025-11-25`-Transport** — nicht ein Dauer-
   Doppelbetrieb. Während der Migration kann `/rpc` als Übergangs-Schritt
   stehenbleiben, doch die Konsumenten (ansV, syllogismus-fedlex) werden auf den
   neuen Endpoint nachgezogen (Runbook 55_MIGRATION §0.4); Rückwärtskompatibilität
   ist **kein Ziel**. — *grösster Brocken.*

2. **Handshake/Negotiation (§B-1).** `initialize`: Client sendet Version; Server
   antwortet bei Support mit **derselben**, sonst mit eigener neuester. Single
   Source of Truth: `SUPPORTED_PROTOCOL_VERSIONS`.
3. **Auth (§B-3).** Resource-Server-Klassifikation + optional Protected-Resource-
   Metadata/`WWW-Authenticate`/OIDC-Discovery — **additiv**, fail-closed (ADR-002)
   unangetastet.
4. **Lifecycle (§B-4).** `notifications/initialized` akzeptieren, `ping`
   beantworten (SHOULD→MUST).
5. **Schema/Capabilities (§B-5).** `tools`-Capability ehrlich; `inputSchema` auf
   **JSON Schema 2020-12** prüfen; optional `title`/`description`/Tool-Annotations.
6. **Bewusst ausgeschlossen (§B-6):** Elicitation, Resources/Prompts, Tasks,
   strukturierte Outputs (vorerst), Icons, Sampling — begründet, analog der
   Lexikon-Projektion (kein Feature ohne Bedarf und Test).

> **Header-Regel (verifiziert, `2025-11-25`):** Fehlt der `MCP-Protocol-Version`-
> Header, **SHOULD** der Server `2025-03-26` annehmen — das ist unsere additive
> Brücke für den heutigen ansV-Pfad ohne `initialize`, bis Phase 6/7 (Runbook 55)
> ansV nachzieht.
>
> **⚠ Reconciliation (zwei Fallback-Ebenen, in Phase 3↔6 abzustimmen):** Dieser
> Header-Fallback (`2025-03-26`) ist **nicht** identisch mit dem Negotiation-Default
> für eine fehlende **`initialize`-`protocolVersion`** (`2024-11-05`, B-1/Runbook 2.2–2.3).
> Beide Ebenen — HTTP-Header **und** Handshake-Body — müssen beim Doppelpfad-Test
> (Runbook 3.4) und beim Default-Flip (6.2) bewusst aufeinander abgestimmt werden,
> sonst entsteht ein stiller Versions-Mismatch zwischen Transport- und Handshake-Ebene.


## §B — Umsetzung (nach §A)

Reihenfolge bewusst: Handshake → Transport → Auth → optionale Features.

- [ ] **B-1 Versions-Negotiation.** `SUPPORTED_PROTOCOL_VERSIONS`-Konstante;
      `initialize` wählt die höchste gemeinsame Version, lehnt unbekannte
      Client-Versionen sauber ab (statt still die eigene zu erzwingen).
- [ ] **B-2 Transport prüfen/anpassen.** Falls die Ziel-Revision den HTTP+SSE-
      Weg deprecatet (zugunsten „Streamable HTTP"): `/sse`+`/rpc` entsprechend
      anpassen oder beide Wege übergangsweise anbieten. Health-Endpunkte
      (`/livez`,`/readyz`,`/startupz`) bleiben unberührt.
- [ ] **B-3 Auth-Mapping.** Unser Bearer/JWT-Modell (ADR-002) gegen das in der
      Ziel-Revision spezifizierte Auth-Modell (z. B. OAuth2-Resource-Server)
      abgleichen; Lücken dokumentieren, fail-closed bleibt unverhandelbar.
      > **✅ Gap-Analyse erledigt (2026-06-21, Runbook 55 §4.1, code-belegt gegen
      > `auth.rs`+`transport.rs`):** Der Reader ist faktisch **bereits** ein OAuth-
      > Resource-Server — `JwtAuthResolver`/`JwksAuthResolver` validieren extern
      > ausgestellte Bearer-JWTs (Issuer Pflicht, Audience optional, `exp`, Rollen-
      > Whitelist, JWKS-Rotation, fail-closed). **Substanz erfüllt**, nur nicht
      > „beworben". **Echte Lücke** ist rein additiv: heute liefert ein fehlendes/
      > ungültiges Credential **HTTP 200 + `-32001` im Body** (kein **401**, kein
      > `WWW-Authenticate`), und es gibt **kein** `.well-known/oauth-protected-resource`
      > (RFC 9728). **Designbefund:** Der 200+Body-Pfad ist genau das, was die zwei
      > header-losen Alt-Clients (ansV, syllogismus-fedlex) brauchen → **401 +
      > `WWW-Authenticate`** gehören an den **neuen** Streamable-HTTP-Pfad (B-2/Phase 3),
      > **nicht** an Legacy-`/rpc`. **Bewusst ausgeschlossen:** OIDC-/AS-Discovery,
      > Scope-Consent (kein OAuth-Scope-Modell — RBAC läuft über die Rolle),
      > Client-ID-Metadata (client-seitig). Damit ist **keine** Pflicht-Lücke offen,
      > die einen harten Schnitt erzwingt; Konformität ist additiv erreichbar,
      > fail-closed (ADR-002) bleibt unangetastet.

- [ ] **B-4 Lifecycle-Notifications.** `notifications/initialized` und `ping`
      ergänzen, falls die Ziel-Revision sie für Konformität verlangt.
      > **⚠ Code-Vorprüfung (2026-06-20): Notification-Handling fehlt vollständig.**
      > `JsonRpcRequest.id` ist `#[serde(default)] Value` → eine **Notification** (JSON-RPC ohne
      > `id`) deserialisiert zu `id = Value::Null`, und `McpService::handle` gibt auf **jedem**
      > Pfad eine `JsonRpcResponse` zurück. Für `notifications/initialized` (heute unbekannte
      > Methode) wäre das ein **`-32601`-Error** — JSON-RPC 2.0 **und** MCP verlangen jedoch, dass
      > auf Notifications (kein `id`) **gar keine** Antwort gesendet wird. **Folge:** B-4 muss (a)
      > Notifications erkennen (fehlendes `id`-Feld vs. `id: null` sauber unterscheiden — dafür `id`
      > als `Option<Value>` modellieren, da `#[serde(default)]` beide Fälle heute kollabiert) und
      > (b) bei Notifications still bleiben (kein Response-Body). `ping` dagegen ist eine echte
      > Request/Response (mit `id`) und nur ein zusätzlicher Match-Arm.
- [ ] **B-5 Capabilities ehrlich deklarieren.** Nur ankündigen, was getestet ist;
      `tools`-Capability ggf. um Flags der neuen Revision erweitern.
      > **Code-Vorprüfung (2026-06-20, entlastet B-5):**
      > - **Schema-Dialekt (#17):** **kein** Tool deklariert `$schema`; alle `schema()`-Methoden
      >   liefern reine `{"type":"object","properties":…,"description":…}`-Objekte
      >   (`discovery.rs`, `metadata.rs`, `tools.rs`). Diese Konstrukte sind über alle
      >   JSON-Schema-Drafts hinweg **identisch** → der 2020-12-Default bringt **kein Wire-Impact**,
      >   nur eine optionale Klarstellung (ggf. `$schema` explizit setzen). Keine Migration nötig.
      > - **Tool-Namen (#4):** durchgängig beschreibendes `snake_case` (`search_law`,
      >   `read_article`, `get_metadata`, `resolve_consolidation_at`, …) → bereits guidance-konform,
      >   **kein** Handlungsbedarf.
      > - **⚠ Feldname `schema` statt `inputSchema` (Konformitätslücke, schon heute):** `tools/list`
      >   emittiert pro Eintrag `{"name", "schema"}` (`registry.rs::list_tools`), während die
      >   MCP-Spec über **alle** Revisionen **`inputSchema`** verlangt. Baseline-Test 1.2 friert das
      >   abweichende Feld bewusst ein. **Folge:** Die Korrektur (`schema` → `inputSchema`) ist Teil
      >   der Konformität, aber **breaking** für beide Alt-Clients, die heute `schema` lesen —
      >   daher als **additiver Doppel-Output** (`inputSchema` *und* übergangsweise `schema`)
      >   auszuliefern, parallel zur Transport-Migration (Phase 3/7), und erst in Phase 9 zu
      >   bereinigen.
      >   **Verifiziert (2026-06-20): ansV liest `schema` aktiv.** `tool_def_from_mcp`
      >   (`ansv-fedlex/src/llm.rs`) zieht `entry.get("schema")` und reicht es als `parameters`
      >   (inkl. `description`) der LLM-Function-Definition durch. Ein **Ersetzen** `schema`→`inputSchema`
      >   ohne Übergang ergäbe bei ansV `parameters = null` → das Modell verlöre alle Argument-Schemata
      >   und Tool-Beschreibungen. Der Doppel-Output ist damit **zwingend** (nicht optional);
      >   syllogismus-fedlex ist unkritisch (ruft nur `tools/call`, nie `tools/list`). Die saubere
      >   Endstufe verlangt zusätzlich ein ansV-Update, das **beide** Felder (`inputSchema` bevorzugt,
      >   `schema` als Fallback) liest — **vor** der Phase-9-Bereinigung.
      >
      >   **✅ UMGESETZT (2026-06-20, Runbook 7.2a):** Der additive Doppel-Output ist live im Code.
      >   **Server** (`registry.rs::list_tools`) emittiert pro Eintrag jetzt **`inputSchema` *und*
      >   `schema`** mit identischem Wert; Baseline-Test 1.2 prüft Präsenz **und** Wertgleichheit
      >   beider Felder. **Client** (`ansv-fedlex/src/llm.rs::tool_def_from_mcp`) liest
      >   **`inputSchema` bevorzugt, `schema` als Fallback** (zwei Unit-Tests decken beide Pfade);
      >   der E2E-Mock (`e2e_belegkette.rs`) spiegelt die Doppel-Form. Damit ist der Alt-Feldname
      >   `schema` erst in **Phase 9** gefahrlos entfernbar — die Konformitätslücke ist additiv
      >   geschlossen, ohne Alt-Clients zu brechen.

- [ ] **B-6 Optionale Features bewerten.** Strukturierte Tool-Outputs,
      Elicitation, Resources/Prompts: pro Feature Nutzen vs. Aufwand; bewusst
      ausschliessen ist erlaubt (begründet, analog Lexikon-Projektion).

## §C — Verifikation & Rollout

- [ ] **C-1 Konformanztest.** Test, der `initialize` gegen die Ziel-Revision
      prüft (ausgehandelte Version, Pflicht-Capabilities, Lifecycle). Analog
      `tests/lexicon_projection.rs` als Regressionsschutz.
- [ ] **C-2 Provenance-Konsistenz-Test.** Ausgehandelte Version == Badge ==
      ADR-Ziel; bricht CI, wenn die drei divergieren.
- [ ] **C-3 Client-Gegentest.** Gegen mindestens einen echten Client (Claude
      Desktop / ansV) end-to-end: `initialize` → `tools/list` → `tools/call`.
- [ ] **C-4 Docs nachziehen.** README-Badge, README-Abschnitt „An MCP-Client
      anbinden", `CHANGELOG.md` (`v0.2.0`), ggf. `80_DEPLOY.md`.
- [ ] **C-5 Tag.** `v0.2.0` setzen — erst wenn C-1..C-4 grün.

## Konsequenzen

**Positiv.** Ehrliche, aktuelle Konformität; künftige Bumps werden trivial
(Liste erweitern statt Konstante suchen); neue Client-Fähigkeiten nutzbar.

**Negativ / Risiko.** Transport-Deprecation (HTTP+SSE → Streamable HTTP) ist der
grösste Brocken und potenziell breaking für bestehende Clients. **Zielzustand ist
allein der saubere `2025-11-25`-Transport** — kein Dauer-Doppelbetrieb; `/rpc` darf
während der Migration übergangsweise bestehen bleiben, doch die Konsumenten (ansV,
syllogismus-fedlex) werden auf den neuen Pfad nachgezogen, statt den Server an deren
Alt-Verhalten zu binden. Auth-Mapping kann zusätzlichen Aufwand bringen, darf aber
das fail-closed-Prinzip (ADR-002) nicht aufweichen.


**Abgrenzung.** Dieses ADR ändert **keinen** Code; es ist der genehmigte Plan.
Bis `v0.2.0` bleibt der Handshake ehrlich bei `2024-11-05` (= was getestet läuft).

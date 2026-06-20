# ADR-008: MCP-Protokollversion — Upgrade & Versions-Negotiation

- **Status:** Proposed — Migrationsplan, noch nicht umgesetzt
- **Datum:** 2026-06-20
- **Kontext-Artefakt:** `crates/mcp-reader/src/transport.rs` (Handshake), `likec4/`
- **Betrifft:** `mcp-fedlex` (Reader), alle MCP-Clients (Agenten, Claude Desktop, ansV)
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

Wichtig für die Substanz dieses ADR: Die **exakten Deltas der Ziel-Revision sind
zum Zeitpunkt des Schreibens nicht aus erster Hand verifiziert.** Dieser Plan
definiert deshalb ZUERST einen Rechercheschritt (§A) gegen die offizielle
Quelle und erst DANN die Umsetzung. Kein String wird „blind" hochgesetzt.

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

## §A — Spec-Recherche (Gate, MUSS zuerst)

Ohne diesen Schritt geht keine Code-Änderung live.

- [ ] **A-1 Offizielle Quelle ziehen.** Aktuelle Revisionsliste + Changelog von
      `modelcontextprotocol.io` (Specification + `schema/` im
      `modelcontextprotocol`-Repo). Die höchste stabile Revision bestimmen
      (Kandidaten u. a. `2025-03-26`, `2025-06-18`, ggf. neuere).
- [ ] **A-2 Delta-Matrix erstellen.** Pro Revision ab `2024-11-05`: was ist neu,
      geändert, **deprecated**, **breaking**. Tabelle in dieses ADR übernehmen.
- [ ] **A-3 Betroffenheit markieren.** Jede Delta-Zeile auf unseren Footprint
      (§Ist-Zustand) abbilden: betrifft uns / optional / irrelevant.
- [ ] **A-4 Ziel-Revision festlegen** und in `CHANGELOG.md` (`v0.2.0`) vormerken.

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
- [ ] **B-4 Lifecycle-Notifications.** `notifications/initialized` und `ping`
      ergänzen, falls die Ziel-Revision sie für Konformität verlangt.
- [ ] **B-5 Capabilities ehrlich deklarieren.** Nur ankündigen, was getestet ist;
      `tools`-Capability ggf. um Flags der neuen Revision erweitern.
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
grösste Brocken und potenziell breaking für bestehende Clients — daher
Übergangs-Doppelbetrieb in B-2 erwägen. Auth-Mapping kann zusätzlichen Aufwand
bringen, darf aber das fail-closed-Prinzip (ADR-002) nicht aufweichen.

**Abgrenzung.** Dieses ADR ändert **keinen** Code; es ist der genehmigte Plan.
Bis `v0.2.0` bleibt der Handshake ehrlich bei `2024-11-05` (= was getestet läuft).

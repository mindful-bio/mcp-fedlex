# ADR-007 — JOLux-Metadaten-Tools: RBAC-Pool & Quota-Gewicht

- **Status:** Accepted (Plan)
- **Datum:** 2026-06-16
- **Betrifft:** Reader / Registry, RBAC-Pools, Quota, `fedlex-jolux`-Projektion, ansV-Konsum
- **Befund-Grundlage:** [45_GAP_ANALYSIS.md](../45_GAP_ANALYSIS.md) (G-1), [50_ROADMAP_TO_PERFECT.md](../50_ROADMAP_TO_PERFECT.md) (Schritt 2)
- **Verhältnis zu ADR-004 / ADR-006:** **Anwendung**, keine neue Ausnahme. ADR-004 (Norm-
  Provenance) gilt **unverändert** und voll; ADR-006 (Hinweis-Provenance) wird **nicht**
  ausgeweitet. Diese ADR klärt nur die beiden offenen Querschnitt-Fragen für die nächste
  Tool-Tranche: **welcher RBAC-Pool** und **welches Quota-Gewicht**.

## Kontext

Schritt 1 der Roadmap (M11, [ADR-006](ADR-006-discovery-tools-and-hint-provenance.md)) ist
umgesetzt: die drei **Discovery**-Tools liefern aus Titel/SR-Nummer/Thema einen Einstiegs-ELI als
**Hinweis**. Der grösste verbleibende strukturelle Hebel (G-1) ist die Projektion der
**JOLux-Metadaten-Primitive** als MCP-Tools. Sie sind in `fedlex-jolux` implementiert und
live-getestet, aber noch nicht als Tools registriert:

- **Tranche A — Temporal:** `check_in_force` (→ `InForce`), `list_versions` (→ `Vec<Version>`),
  `resolve_consolidation_at` (→ `Consolidation`).
- **Tranche B — Beziehungen:** `get_impacts` / `get_outgoing_impacts` / `get_article_history`
  (→ `Vec<Impact>`), `get_citations` (→ `Vec<Citation>`).
- **Tranche C — Einordnung:** `get_taxonomy` (→ `Vec<TaxonomyEntry>`), `get_subdivisions` /
  `list_annexes`.

Zwei Eigenschaften dieser Primitive sind für die Entscheidung wesentlich (am Code verifiziert):

1. **Sie tragen bereits Norm-Provenance.** Jedes Primitiv gibt `Result<Response<T>, JoluxError>`
   zurück und konstruiert die `Response` über `Provenance::new(eli, as_of, TransactionTime::now())`
   — also `kind: "norm"`. Anders als Discovery liest jedes dieser Tools eine Eigenschaft eines
   **bekannten** Erlasses (Eingabe ist ein ELI), nicht einen Suchtreffer. Das ADR-004-Gate ist
   damit schon in der `fedlex-jolux`-Schicht erfüllt; der Tool-Wrapper reicht die `Response` nur
   durch.
2. **Sie gehen live an den öffentlichen Fedlex-SPARQL-Endpoint** — wie Discovery, anders als die
   neun bestehenden Navigations-Tools, die über den `AknFetcher` (Direct Fetch **mit
   Manifestations-Cache**) laufen.

## Entscheidung

### 1. Provenance: Norm (keine Ausnahme)

Diese Tools sind **norm-provenance-tragend**. Sie übernehmen die `Provenance` der zugrunde
liegenden `Response<T>` unverändert (`kind: "norm"`). ADR-006 (`Hint`) ist hier **nicht**
anwendbar: Eingabe ist ein bekanntes ELI, die Antwort ist eine belegte Aussage über genau diesen
Erlass zum Stichtag, kein Kandidat. *gefunden* vs. *belegt* bleibt sauber getrennt.

### 2. RBAC: neuer Pool `JoluxMetadata`

Diese Tools erhalten einen **eigenen Pool** `ToolPool::JoluxMetadata`, getrennt von
`LocalNavigation` und `Discovery`. Begründung:

- **Nicht `LocalNavigation`.** Dieser Pool meint Lesen aus dem **lokalen Manifestations-Cache**
  (AknFetcher). Die Metadaten-Tools haben kein Dokument und keinen Cache, sie gehen live gegen
  SPARQL — anderes Lastprofil, andere Fehlermodi. Sie unter `LocalNavigation` zu führen, würde die
  Bedeutung des Pools (und das Quota-Gewicht, s.u.) verwässern.
- **Nicht `Discovery`.** Discovery findet *erst* einen Erlass und liefert **Hinweise**;
  Metadaten-Tools belegen Eigenschaften eines *bekannten* Erlasses und liefern **Norm**. Ein
  gemeinsamer Pool würde zwei unterschiedliche Provenance-Bedeutungen unter einem
  Sichtbarkeits-/Quota-Schalter mischen.

Sichtbarkeit (analog Discovery, Least-Privilege):

- `Reader`: **kein** Zugriff (bleibt eng auf den lokalen Cache).
- `Navigator`: **ja** (das ist die Rolle, mit der `ansV` läuft).
- `Validator`: **ja**.

### 3. Quota: gleiches Live-Gewicht wie Discovery

`JoluxMetadata` bekommt dasselbe erhöhte Cost-Gewicht wie `Discovery`
(`cost_weight() = 5`, vs. `1` für lokale Navigation). Begründung: identisches Lastprofil (Live-
Last gegen den öffentlichen Endpoint). Das Gewicht bleibt **claim-/pool-gebunden** und ist von
keinem LLM-Parameter senkbar (ADR-002). Sollte sich die Last der beiden Pools später messbar
unterscheiden, kann das Gewicht pro Pool getrennt nachgezogen werden — die `cost_weight`-API ist
bereits pool-granular.

## Akzeptanzkriterien (Code)

- [x] **Pool existiert.** `ToolPool::JoluxMetadata` ist definiert; `pools_for` gibt ihn für
      `Navigator` und `Validator`, **nicht** für `Reader` frei.
      (`tool.rs::tests::jolux_metadata_visibility_follows_least_privilege`)
- [x] **Quota-Gewicht.** `ToolPool::JoluxMetadata.cost_weight()` == `ToolPool::Discovery.cost_weight()`
      (> `LocalNavigation`); ein Metadaten-Call bucht das schwerere Gewicht (Transport-Test).
      (`tool.rs::tests::jolux_metadata_costs_same_as_discovery_and_more_than_local`,
      `transport.rs::tests::jolux_metadata_call_books_weighted_quota_like_discovery`)
- [x] **RBAC-Negativtest.** `Reader`-`tools/call` auf z.B. `check_in_force` wird graceful
      abgewiesen (`not permitted`).
      (`metadata.rs::tests::reader_call_on_check_in_force_is_gracefully_denied`)
- [x] **Norm-Provenance im Wire-Format.** Ein Dispatch-Test zeigt `kind: "norm"` und das
      angefragte ELI in `provenance.eli` (kein `hint`).
      (`metadata.rs::tests::check_in_force_carries_norm_provenance_for_requested_eli` u.a.)
- [x] **Tranche A projiziert.** `check_in_force`, `list_versions`, `resolve_consolidation_at`
      sind in `tools/list` rollenabhängig sichtbar und über `tools/call` aufrufbar; je ein
      Mock-Dispatch-Test reicht das Primitiv durch.
      (`metadata.rs::tests`, in `main.rs` via `register_metadata_tools` verdrahtet)
- [x] **Tranche B projiziert.** Beziehungen (`get_impacts`/`get_outgoing_impacts`/
      `get_article_history`, `get_citations`) sind in `tools/list` rollenabhängig sichtbar und
      über `tools/call` aufrufbar; Mock-Dispatch-Tests reichen die Primitive durch und prüfen
      Norm-Provenance, eID-Normalisierung (J18.2), `(from,to)`-Dedup (J7.4) und `direction`.
      (`metadata.rs::tests`, in `main.rs` via `register_metadata_tools` verdrahtet)
- [x] **Tranche C projiziert.** Einordnung (`get_taxonomy`, `get_subdivisions`/`list_annexes`)
      ist in `tools/list` rollenabhängig sichtbar und über `tools/call` aufrufbar;
      Mock-Dispatch-Tests reichen die Primitive durch und prüfen Norm-Provenance,
      Sprach-Filter (`get_taxonomy`), den optionalen `type_uri`-Filter und den
      Annex-Spezialfall (`subdivision-type/annex`, JLX-SUB-02).
      (`metadata.rs::tests`, in `main.rs` via `register_metadata_tools` verdrahtet)

- [ ] **Vollständigkeits-Matrix.** Jedes projizierte Primitiv ist in der Matrix aus
      [50_ROADMAP_TO_PERFECT.md](../50_ROADMAP_TO_PERFECT.md) (Schritt 3) als *projiziert*
      verbucht.



## Alternativen

- **Wiederverwendung von `Discovery`.** Verworfen. Mischt Hinweis- und Norm-Bedeutung unter einem
  Pool; erschwert spätere getrennte Quota-/Sichtbarkeits-Politik.
- **Wiederverwendung von `LocalNavigation`.** Verworfen. Falsches Lastprofil (Live-SPARQL statt
  Cache-Read); würde das niedrige Navigations-Gewicht auf Live-Calls anwenden und den externen
  Endpoint unterschätzen.
- **Pro Tranche ein eigener Pool.** Verworfen (vorerst). Überspezifiziert ohne belegten Bedarf;
  ein gemeinsamer `JoluxMetadata`-Pool genügt, solange Sichtbarkeit und Last gleich sind.
- **Eigenes, höheres Gewicht als Discovery.** Verworfen mangels Messung. Gleiches Lastprofil →
  gleiches Gewicht; bei Bedarf später pool-granular nachziehbar.

## Konsequenzen

- **Positiv.** Die gutachtenkritischen Fragen („in Kraft zum Stichtag?", „welche Fassungen?",
  „was ändert/zitiert?") werden norm-belegt verfügbar; die Discovery→Beleg-Kette aus ADR-006
  schliesst sich (Hinweis-ELI → Metadaten-Tool mit Norm-Provenance). Minimaler Eingriff: dünne
  Wrapper, ein neuer Pool, ein Mapping-Eintrag im Quota-Gewicht.
- **Negativ.** Ein weiterer Pool erhöht die Last gegen Fedlex (durch das Discovery-Gewicht
  gedämpft). Die `tools/list`-Oberfläche wächst spürbar — die Tranchen sollten daher in der
  Reihenfolge A→B→C nach belegtem ansV-Bedarf projiziert werden.
- **Modell.** Neuer `ToolPool::JoluxMetadata` in der LikeC4-Spec; neue Tool-Knoten unter dem
  Reader (gruppiert nach Tranche).

---

## Status der Umsetzung
**Tranchen A, B und C vollständig umgesetzt** (`crates/mcp-reader/src/metadata.rs`):
Tranche A (Temporal) — `check_in_force`, `list_versions`, `resolve_consolidation_at`;
Tranche B (Beziehungen) — `get_impacts`, `get_outgoing_impacts`, `get_article_history`,
`get_citations`; Tranche C (Einordnung) — `get_taxonomy`, `get_subdivisions`, `list_annexes`.
Alle zehn Tools als `ToolPool::JoluxMetadata`, via `register_metadata_tools` in `main.rs`
verdrahtet; alle obigen Akzeptanzkriterien für A, B und C testgrün
(`cargo test -p mcp-reader`, 125 Tests). Damit ist G-1 (Projektion der JOLux-Metadaten-Schicht)
geschlossen; offen bleibt nur die Vollständigkeits-Matrix (Roadmap Schritt 3) als Rückfall-Schutz.




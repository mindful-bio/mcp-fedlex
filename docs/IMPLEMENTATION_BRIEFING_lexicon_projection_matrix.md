# Implementierungs-Briefing — Vollständigkeits-Matrix (Roadmap Schritt 3 / ADR-007 letztes Kriterium)

> **Zweck.** Schliesst den letzten offenen Punkt aus `45_GAP_ANALYSIS.md` (G-4) und das letzte
> offene Akzeptanzkriterium in `adr/ADR-007-…md` (Z. 108–110): einen **Offline-CI-Test**, der
> garantiert, dass **jedes** im Lexikon dokumentierte Primitiv genau einem Zustand zugeordnet ist:
> **`Projected`** (als MCP-Tool registriert) **oder** **`Excluded`** (begründeter Ausschluss).
> Der Test wird **rot**, sobald ein neues Lexikon-Primitiv ohne Zuordnung dazukommt — Schutz gegen
> ein erneutes, unbemerktes Auseinanderlaufen von Lexikon und Tool-Oberfläche.
>
> Stand der Recherche: 2026-06-16, alles am Code/an den Docs verifiziert.

---

## 1. Ausgangslage (verifiziert)

### 1.1 Soll: 47 Lexikon-IDs

Quelle: Markdown-Header `### <ID> · <funktionsname>` in
- `docs/10_LEXICON_jolux.md` (27 IDs)
- `docs/11_LEXICON_akn.md` (20 IDs)

Extraktions-Regex (in Rust gegen den eingelesenen Dateiinhalt):
```
^### ((?:JLX|AKN)-[A-Z]+-[0-9]+) · ([a-z_]+)
```
Trennzeichen ist ein **Mittelpunkt `·` (U+00B7)**, umgeben von je einem Leerzeichen.

Vollständige Soll-Liste (ID = Lexikon-Funktionsname):

**JOLux (27):**
```
JLX-RES-01 resolve_sr_number      JLX-RES-02 search_law            JLX-RES-03 get_law_metadata
JLX-RES-04 resolve_manifestation  JLX-RES-05 list_expressions      JLX-TMP-01 list_versions
JLX-TMP-02 resolve_version_at     JLX-TMP-03 check_in_force        JLX-SUB-01 get_subdivisions
JLX-SUB-02 list_annexes           JLX-IMP-01 get_impacts           JLX-IMP-02 get_article_history
JLX-IMP-03 get_outgoing_impacts   JLX-CIT-01 get_citations         JLX-TAX-01 get_taxonomy
JLX-TAX-02 find_related_by_topic  JLX-PUB-01 get_oc_act            JLX-PUB-02 get_memorial
JLX-PUB-03 get_fga_documents      JLX-GEN-01 get_drafts            JLX-GEN-02 get_consultations
JLX-GEN-03 get_consultation_documents  JLX-TRT-01 get_treaty_info  JLX-TRT-02 find_treaties
JLX-VOC-01 resolve_vocabulary_term     JLX-VOC-02 list_vocabulary  JLX-VOC-03 explore_node
```

**AKN (20):**
```
AKN-DOC-01 fetch_akn_document     AKN-DOC-02 get_frbr_metadata     AKN-DOC-03 classify_pattern
AKN-STR-01 get_document_structure AKN-STR-02 resolve_eid           AKN-STR-03 get_section_path
AKN-TXT-01 get_article_text       AKN-TXT-02 get_element_text      AKN-TXT-03 get_readable_document
AKN-TXT-04 search_text            AKN-MOD-01 get_modifications     AKN-MOD-02 extract_change_notes
AKN-REF-01 get_all_references     AKN-REF-02 parse_unlinked_refs   AKN-CMP-01 list_components
AKN-CMP-02 get_component_document AKN-SPC-01 extract_tables        AKN-SPC-02 detect_foreign_content
AKN-CHK-01 hollow_document        AKN-CHK-02 chunk_document
```

### 1.2 Ist: 22 registrierte MCP-Tools (21 projiziert + 1 Composite)

> **Korrektur gegenüber Erstfassung (durch Test verifiziert).** Von den 22
> registrierten Tools haben **21** ein 1:1-Lexikon-Primitiv (→ Matrix §2). Das
> 22. Tool, **`compare_versions`**, ist ein **Composite ohne Lexikon-ID** (Diff aus
> `get_document_structure` + `get_element_text`) und liegt im Pool
> **`Validation`** — es taucht daher in keiner lexikon-ID-getriebenen Matrix auf
> und wird im Test über die `COMPOSITE_TOOLS`-Allowlist verbucht. Das Lexikon ist
> laut eigenem Vorwort *„kein Tool-Katalog … sondern das Vokabular, aus dem
> Konsumenten komponieren"* — Composite-Tools sind also legitim und erwartbar.

Exakte `name()`-Strings, aus den drei `register_*`-Funktionen verifiziert:

- **Navigation** (`crates/mcp-reader/src/tools.rs`, `register_navigation_tools`, 9):
  `read_article, read_element, get_structure, search_text, get_metadata,
   read_document, get_references, get_modifications, compare_versions`
  (davon ist `compare_versions` Pool `Validation`, kein Lexikon-Primitiv — Composite)
- **Metadaten** (`crates/mcp-reader/src/metadata.rs`, `register_metadata_tools`, 10):
  `check_in_force, list_versions, resolve_consolidation_at, get_impacts,
   get_outgoing_impacts, get_article_history, get_citations, get_taxonomy,
   get_subdivisions, list_annexes`
- **Discovery** (`crates/mcp-reader/src/discovery.rs`, `register_discovery_tools`, 3):
  `search_law, resolve_sr_number, find_related_topic`

> **Wichtige Namens-Fallen (sonst läuft die Matrix falsch):**
> - **Tool-Name ≠ Lexikon-Funktionsname** in drei Fällen:
>   - `JLX-TMP-02` Lexikon `resolve_version_at` → Tool **`resolve_consolidation_at`**
>   - `JLX-TAX-02` Lexikon `find_related_by_topic` → Tool **`find_related_topic`** (ohne `by_`)
>   - `AKN-TXT-03` Lexikon `get_readable_document` → Tool **`read_document`**
> - `get_metadata` (Tool) deckt laut Doc-Kommentar **`AKN-DOC-02/03`** ab (FRBR + `classify_pattern`).
>   Das Tool nutzt intern `classify_pattern` (AKN-DOC-03), exponiert es aber nicht als eigenes Tool.
> - `get_modifications`/`search_text`/`get_references`/`get_structure`/`read_article`/`read_element`
>   bilden 1:1 auf die AKN-Lexikon-Funktionen ab (`get_modifications`, `search_text`,
>   `get_all_references`, `get_document_structure`, `get_article_text`, `get_element_text`).

### 1.3 Die Matrix darf NICHT auf Tool-Namen-Gleichheit prüfen

Wegen der Namens-Fallen oben **muss** der Test eine **explizite Mapping-Tabelle Lexikon-ID →
Zustand** im Testcode führen (Single Source of Truth des Tests). Die Registry liefert nur die
*Menge der tatsächlich registrierten Tool-Namen*; der Test prüft Konsistenz zwischen
Mapping-Tabelle, Lexikon-Datei und Registry — siehe §3.

---

## 2. Soll-Matrix (die Mapping-Tabelle für den Test)

Zustand pro ID. `Projected("<tool_name>")` = über MCP erreichbar; `Excluded("<grund>")` =
bewusster, begründeter Ausschluss.

### 2.1 JOLux

| ID | Zustand | Tool-Name / Begründung |
|----|---------|------------------------|
| JLX-RES-01 | Projected | `resolve_sr_number` (Discovery) |
| JLX-RES-02 | Projected | `search_law` (Discovery) |
| JLX-RES-03 | Excluded  | Steckbrief; vom Reader intern genutzt, kein eigenständiges Agenten-Tool (offen für spätere Tranche, vgl. ADR-007 Konsequenzen) |
| JLX-RES-04 | Excluded  | `resolve_manifestation` — interner Schritt der Bridge (`AknFetcher`), nicht als Tool exponiert |
| JLX-RES-05 | Excluded  | `list_expressions` — Sprachvarianten-Auflösung, intern; kein belegter Agenten-Bedarf |
| JLX-TMP-01 | Projected | `list_versions` (Metadaten) |
| JLX-TMP-02 | Projected | `resolve_consolidation_at` (Metadaten) — **Name weicht ab** |
| JLX-TMP-03 | Projected | `check_in_force` (Metadaten) |
| JLX-SUB-01 | Projected | `get_subdivisions` (Metadaten) |
| JLX-SUB-02 | Projected | `list_annexes` (Metadaten) |
| JLX-IMP-01 | Projected | `get_impacts` (Metadaten) |
| JLX-IMP-02 | Projected | `get_article_history` (Metadaten) |
| JLX-IMP-03 | Projected | `get_outgoing_impacts` (Metadaten) |
| JLX-CIT-01 | Projected | `get_citations` (Metadaten) |
| JLX-TAX-01 | Projected | `get_taxonomy` (Metadaten) |
| JLX-TAX-02 | Projected | `find_related_topic` (Discovery) — **Name weicht ab** |
| JLX-PUB-01 | Excluded  | `get_oc_act` — Publikations-/Verbindlichkeitsschicht, noch nicht projizierte Tranche; kein belegter ansV-Bedarf |
| JLX-PUB-02 | Excluded  | `get_memorial` — dito |
| JLX-PUB-03 | Excluded  | `get_fga_documents` — dito |
| JLX-GEN-01 | Excluded  | `get_drafts` — Entstehungsgeschichte, kein gutachtenkritischer Pfad |
| JLX-GEN-02 | Excluded  | `get_consultations` — dito |
| JLX-GEN-03 | Excluded  | `get_consultation_documents` — dito |
| JLX-TRT-01 | Excluded  | `get_treaty_info` — Staatsverträge, eigene spätere Tranche |
| JLX-TRT-02 | Excluded  | `find_treaties` — dito |
| JLX-VOC-01 | Excluded  | `resolve_vocabulary_term` — kontrolliertes Vokabular, kein direkter Agenten-Bedarf |
| JLX-VOC-02 | Excluded  | `list_vocabulary` — dito |
| JLX-VOC-03 | Excluded  | `explore_node` — dito |

### 2.2 AKN

| ID | Zustand | Tool-Name / Begründung |
|----|---------|------------------------|
| AKN-DOC-01 | Excluded  | `fetch_akn_document` — produktive Bridge-Komposition, **interner** Fetch, kein eigenes Tool |
| AKN-DOC-02 | Projected | `get_metadata` (Navigation; deckt DOC-02 ab) |
| AKN-DOC-03 | Excluded  | `classify_pattern` — intern in `get_metadata` genutzt, nicht eigenständig exponiert |
| AKN-STR-01 | Projected | `get_structure` (Navigation) |
| AKN-STR-02 | Excluded  | `resolve_eid` — interner Lookup, kein Agenten-Tool |
| AKN-STR-03 | Excluded  | `get_section_path` — interner Pfad-Helper |
| AKN-TXT-01 | Projected | `read_article` (Navigation) |
| AKN-TXT-02 | Projected | `read_element` (Navigation) |
| AKN-TXT-03 | Projected | `read_document` (Navigation) — **Name weicht ab** |
| AKN-TXT-04 | Projected | `search_text` (Navigation) |
| AKN-MOD-01 | Projected | `get_modifications` (Navigation) |
| AKN-MOD-02 | Excluded  | `extract_change_notes` — Nutzwert-Lücke (G-2), bis auf Weiteres ausgeschlossen |
| AKN-REF-01 | Projected | `get_references` (Navigation) |
| AKN-REF-02 | Excluded  | `parse_unlinked_refs` — Nutzwert-Lücke (G-2), ausgeschlossen |
| AKN-CMP-01 | Excluded  | `list_components` — Nutzwert-Lücke (G-2), ausgeschlossen |
| AKN-CMP-02 | Excluded  | `get_component_document` — dito |
| AKN-SPC-01 | Excluded  | `extract_tables` — Nutzwert-Lücke (G-2), ausgeschlossen |
| AKN-SPC-02 | Excluded  | `detect_foreign_content` — Nutzwert-Lücke (G-2), ausgeschlossen |
| AKN-CHK-01 | Excluded  | `hollow_document` — **RAG-Ingest** für `semantic-fedlex`, bewusst kein Agenten-Tool (Lexikon §CHK) |
| AKN-CHK-02 | Excluded  | `chunk_document` — dito |

**Zusammenfassung:** 21 `Projected`, 26 `Excluded`, Summe 47. (JOLux 13/14, AKN 8/12.)
Plus **1 Composite-Tool** (`compare_versions`) ohne Lexikon-Primitiv ⇒ **22
registrierte MCP-Tools** insgesamt.

> Die `Excluded`-Begründungen für G-2 (`MOD-02`, `REF-02`, `CMP-*`, `SPC-*`) sind als
> *Nutzwert-Lücke, bewusst zurückgestellt* zu lesen — `45_GAP_ANALYSIS.md` empfiehlt, sie
> explizit als Ausschluss festzuhalten (genau das tut diese Matrix). `CHK-*` sind echte,
> dauerhafte Ausschlüsse (RAG-Schicht).

---

## 3. Test-Spezifikation

**Datei:** `crates/mcp-reader/tests/lexicon_projection.rs` (neue Integrationstest-Datei,
analog `quota_integration.rs`). **Kein `#[ignore]`** — der Test ist offline und soll in jeder
CI-Runde laufen.

### 3.1 Imports / verfügbare Bausteine (alle aus crate-root re-exportiert, verifiziert)

```rust
use mcp_reader::{
    register_navigation_tools, register_metadata_tools, register_discovery_tools,
    Registry, Role,
};
// Mocks:
use fedlex_jolux::MockSparqlClient;          // ::from_json(&str) | ::new(SparqlResults)
use fedlex_bridge::{AknFetcher, MockXmlSource}; // AknFetcher::new(sparql, xml_source, cache_cap)
use std::sync::Arc;
```

Die `register_*`-Signaturen (verifiziert):
```rust
register_navigation_tools<C, S>(&mut Registry, Arc<AknFetcher<C, S>>)   // C: SparqlClient, S: XmlSource
register_metadata_tools<C>(&mut Registry, Arc<C>)                       // C: SparqlClient
register_discovery_tools<C>(&mut Registry, Arc<C>)                      // C: SparqlClient
```

`MockSparqlClient::from_json` und `MockXmlSource::new` brauchen nur *irgendeinen* gültigen
Inhalt — der Test ruft die Tools **nicht auf**, er prüft nur *welche registriert sind*. Ein
leeres bzw. minimales Fixture genügt (z. B. `MINI_ACT`/`CONS_JSON` aus den `tools.rs`-Tests
übernehmen, oder triviale Stubs; Hauptsache `from_json` parst).

### 3.2 Wie man die registrierten Tool-Namen erhält

`Registry` exponiert `list_tools(role: Role) -> Vec<serde_json::Value>` (jedes Element hat ein
Feld `"name"`). Mit `Role::Validator` sind **alle** Pools sichtbar (Navigator+Validator sehen
`LocalNavigation`, `Discovery`, `JoluxMetadata`; `Reader` nur `LocalNavigation`). Also:

```rust
fn registered_tool_names() -> std::collections::BTreeSet<String> {
    let mut r = Registry::new();
    let fetcher = Arc::new(AknFetcher::new(
        MockSparqlClient::from_json(CONS_JSON),
        MockXmlSource::new(MINI_ACT),
        8,
    ));
    register_navigation_tools(&mut r, fetcher);
    register_metadata_tools(&mut r, Arc::new(MockSparqlClient::from_json("{...}")));
    register_discovery_tools(&mut r, Arc::new(MockSparqlClient::from_json("{...}")));
    r.list_tools(Role::Validator)
        .into_iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect()
}
```
> **Korrektur (durch Test verifiziert):** `Validator` sieht **alle** Pools inkl.
> `Validation`. Es ist **ein** Validation-Tool registriert — das Composite
> `compare_versions`. Das Set enthält also die 21 projizierten Lexikon-Tools **plus**
> `compare_versions` = 22. Der Test verbucht `compare_versions` über die
> `COMPOSITE_TOOLS`-Allowlist (Assertion 3), nicht über die Matrix.

### 3.3 Die Mapping-Tabelle im Test

```rust
enum Projection { Projected(&'static str), Excluded(&'static str) }
use Projection::*;

// (Lexikon-ID, Zustand) — die Matrix aus §2, als const-Array (47 Einträge).
const MATRIX: &[(&str, Projection)] = &[
    ("JLX-RES-01", Projected("resolve_sr_number")),
    // … alle 47 Einträge exakt aus §2 …
    ("AKN-CHK-02", Excluded("RAG-Ingest für semantic-fedlex")),
];

// Registrierte Tools OHNE Lexikon-Primitiv (Composite). Müssen bewusst
// verbucht sein; ein neues, nicht eingetragenes Composite lässt Assertion 3 brechen.
const COMPOSITE_TOOLS: &[&str] = &["compare_versions"];
```

### 3.4 Die sechs Assertions (das Herz des Tests)

> **Härtung gegenüber Erstfassung.** Die ursprünglich vier Assertions prüften nur
> *welche* Tools registriert sind. Da der Sonderstatus von `compare_versions` aber
> ganz an seinem **Pool** (`Validation`) und seiner **Rollen-Sichtbarkeit** hängt,
> sind zwei weitere Assertions ergänzt (5 + 6), die genau das maschinell festnageln —
> sonst wäre die „Composite-Tool im Validation-Pool, nur Validator sieht es"-Aussage
> reine Prosa.


1. **Lexikon ↔ Matrix deckungsgleich.**
   Lies beide Lexikon-Dateien (`env!("CARGO_MANIFEST_DIR")` + `"/../../docs/10_LEXICON_jolux.md"`
   bzw. `…/11_LEXICON_akn.md`), extrahiere alle IDs via Regex (§1.1).
   - Jede Lexikon-ID **muss** in `MATRIX` vorkommen → sonst „neues Primitiv ohne Zuordnung"
     (das ist der G-4-Schutz; diese Assertion wird rot bei neuem Lexikon-Eintrag).
   - Jede `MATRIX`-ID **muss** im Lexikon vorkommen → kein veralteter Matrix-Eintrag.
   - Erwartete Gesamtzahl assertieren: `assert_eq!(ids.len(), 47)` (Drift-Anker).

2. **Jedes `Projected` ist real registriert.**
   Für jeden `Projected(tool)` in `MATRIX`: `assert!(registered.contains(tool))`.

3. **Kein registriertes Tool ist „verwaist".**
   Sammle alle `Projected`-Tool-Namen aus `MATRIX` **vereinigt mit `COMPOSITE_TOOLS`**
   in ein Set; assertiere, dass es **gleich** `registered` ist (`assert_eq!`). Fängt
   ein neu registriertes, weder in Matrix noch in `COMPOSITE_TOOLS` verbuchtes Tool ab.

4. **Zähl-Invariante (Doku-Anker).**
   `assert_eq!(projected_count, 21); assert_eq!(excluded_count, 26);`
   `assert_eq!(COMPOSITE_TOOLS.len(), 1); assert_eq!(registered.len(), 22);`
   Macht den dokumentierten Stand testgebunden; ändert sich die Projektion, zwingt der Test zum
   bewussten Update von Matrix **und** Doku.

5. **Pool-Zuordnung stimmt mit der Matrix-Prosa überein** (`Registry::pool_of`).
   Jedes `Projected`-Tool **muss** in einem der „Lexikon-Pools" liegen
   (`LocalNavigation`, `LodFederation`, `Discovery`, `JoluxMetadata`) — **nie** in
   `Validation`/`Workspace`. Jedes `COMPOSITE_TOOLS`-Tool **muss** in genau dem in
   `COMPOSITE_POOL_OF` erwarteten Pool liegen (`compare_versions` → `Validation`).
   Fängt ein versehentlich falsch eingeordnetes Tool ab (z. B. Discovery-Tool in
   `Validation`), das sonst still durchrutschen würde.

6. **RBAC pro Rolle + Monotonie (Least-Privilege).**
   `Reader ⊆ Navigator ⊆ Validator` (jede höhere Rolle sieht mindestens so viel).
   Jedes Composite-Tool im `Validation`-Pool ist **nur** für `Validator` sichtbar,
   **nicht** für `Reader`/`Navigator`. `Reader` sieht eine **echte, nicht-leere**
   Teilmenge (nur der lokale Cache-Lesepfad). Macht den eigentlichen Sinn des
   Composite-Sonderstatus (ADR-007) testgebunden statt nur dokumentiert.

### 3.5 Stil / Konventionen

- Deutschsprachige Doc-Kommentare im Kopf (wie `quota_integration.rs`, `lexicon_conformance.rs`).
- Ein einziger `#[test]` (synchron reicht; `list_tools` ist nicht async) oder `#[tokio::test]`
  falls der `AknFetcher`-Bau das verlangt — `AknFetcher::new` ist nicht async, also **synchroner
  `#[test]` genügt**.
- Aussagekräftige Panic-Messages: bei (1) den fehlenden/überzähligen ID-Namen ausgeben, damit
  der nächste Wartende sofort sieht, *welches* Primitiv neu ist und einen Matrix-Eintrag braucht.

---

## 4. Doku-Updates (nach grünem Test)

1. **`docs/adr/ADR-007-jolux-metadata-tools-pool-and-quota.md`**, Z. 108–110:
   `- [ ] **Vollständigkeits-Matrix.**` → `- [x]` und ergänzen:
   „Verankert in `crates/mcp-reader/tests/lexicon_projection.rs` (Offline, läuft in `cargo test`);
   47 IDs, 21 projiziert / 26 begründet ausgeschlossen, + 1 Composite-Tool
   (`compare_versions`) ⇒ 22 registrierte MCP-Tools."

2. **`docs/50_ROADMAP_TO_PERFECT.md`**, Schritt 3 (Vollständigkeits-Matrix): als erledigt
   markieren, mit Verweis auf die Testdatei. (Den genauen Wortlaut/Checkbox-Stil dort vor dem
   Editieren kurz gegenlesen — Datei war in dieser Session nicht im Detail geöffnet.)

3. **`docs/45_GAP_ANALYSIS.md`**, G-4: Status auf „geschlossen" setzen, Verweis auf die Testdatei.
   Optional G-2 ergänzen: „die fünf AKN-Aufbereitungs-Primitive sind nun **explizit** als
   `Excluded` in der Matrix verbucht (Nutzwert-Lücke, bewusst zurückgestellt)."

---

## 5. Verifikation

```sh
cd mindful.bio/mcp-fedlex
cargo test -p mcp-reader --test lexicon_projection      # neuer Test, offline, muss grün sein
cargo test -p mcp-reader                                # Regression: alle bisherigen 125 grün
```

**Negativ-Probe (manuell, optional):** einen Eintrag aus `MATRIX` löschen → Test muss mit klarer
Meldung rot werden („Lexikon-ID X hat keinen Matrix-Eintrag"). Danach wieder einfügen.

---

## 6. Offene Mini-Entscheidung für die Umsetzung

Die `Excluded`-Begründungen in §2 sind bewusst knapp gehalten. Falls gewünscht, beim Umsetzen
die G-2-Einträge (`MOD-02`, `REF-02`, `CMP-*`, `SPC-*`) einheitlich als
`Excluded("G-2: Nutzwert-Lücke, zurückgestellt")` und die `CHK-*` als
`Excluded("RAG: semantic-fedlex-Ingest")` formulieren — das hält die Tabelle grep-bar nach
Ausschluss-Kategorie.

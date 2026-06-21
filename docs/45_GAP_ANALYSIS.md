# 45 — Reifegrad- & Lücken-Analyse (Tool-Projektion)

> **Was dieses Dokument ist.** Eine ehrliche, am Code verifizierte Bestandsaufnahme: Welche
> Daten-Primitive existieren, welche davon sind als **MCP-Tools** projiziert, und wo klafft eine
> Lücke. Es korrigiert auch eine frühere Fehlannahme in `40_FINDINGS.md`/`50_ROADMAP` (CI).
> Methodik: `grep`/`search_files` über `crates/` + Lektüre von `mcp-reader/src/tools.rs`,
> `fedlex-*/src/lib.rs`, `.github/workflows/`. Stand: 2026-06-16.

---

## 0. Korrektur einer früheren Annahme (CI / Live-Konformanz)

`50_ROADMAP_TO_PERFECT.md` (Schritt 2) behauptete, die Live-Konformanz laufe „nie in CI". **Das
ist falsch.** Es existiert `.github/workflows/live-conformance.yml`:

- Trigger: `schedule: "17 5 * * 1"` (wöchentlich Mo 05:17 UTC) **+** `workflow_dispatch`.
- Fährt **alle drei** Live-Suiten gegen das echte Fedlex: jolux (29), akn (12), bridge (3),
  jeweils `-- --ignored --test-threads 2` (WAF-Schonung), `timeout-minutes: 30`.
- `ci.yml` fährt zusätzlich die Docker-/Redis-Integration (`fedlex-store`, `quota_integration`)
  via `--ignored`.

**Folge:** „Live-Konformanz in CI" ist **kein offener Punkt**. Es bleibt nur eine kleine
Härtung (siehe G-5): wöchentlich statt nightly, und kein Alert/Issue bei rotem Lauf.

---

## 1. Datenschichten und ihr Umfang (Soll)

| Crate | Rolle | öffentl. Primitive (ca.) | Live-Konformanz |
| --- | --- | --- | --- |
| `fedlex-jolux` | JOLux-Metadaten über SPARQL (RES, TMP, IMP, REL, TAX, GEN, PUB, TREATY, VOC, SUB, MET) | ~22 über 12 Module | 29 Live-Tests (wöchentlich CI) |
| `fedlex-akn` | AKN-Volltext/Struktur aus XML | ~41 fns / 11 Familien | 12 Live-Tests (wöchentlich CI) |
| `fedlex-bridge` | Komposition JOLux→AKN (`AknFetcher`, `HttpSparqlClient`, `HttpXmlSource`) | `fetch_akn_document` (AKN-DOC-01 produktiv) | 3 Live-Tests (wöchentlich CI) |

> Die Daten- und Kompositionsschicht ist **breit und live-getestet**. Die Frage ist allein, wie
> viel davon an der **MCP-Tool-Oberfläche** ankommt.

---

## 2. Was der Reader tatsächlich als Tool registriert (Ist)

Es gibt genau **eine** Registrierung: `register_navigation_tools` (`mcp-reader/src/tools.rs`,
aufgerufen in `main.rs`). Sie listet **9 Tools**, alle AKN-Volltext/Struktur, alle mit
Pflicht-`eli`:

| Tool | nutzt AKN-Primitiv | Pool |
| --- | --- | --- |
| `read_article` | `get_article_text` (+ `classify_pattern`) | LocalNavigation |
| `read_element` | `get_element_text` | LocalNavigation |
| `get_structure` | `get_document_structure` | LocalNavigation |
| `search_text` | `search_text` | LocalNavigation |
| `get_metadata` | `get_frbr_metadata` (+ `classify_pattern`) | LocalNavigation |
| `read_document` | `get_readable_document` | LocalNavigation |
| `get_references` | `get_all_references` | LocalNavigation |
| `get_modifications` | `get_modifications` | LocalNavigation |
| `compare_versions` | `get_document_structure` + `get_element_text` (Diff) | LocalNavigation |

`fedlex-jolux` wird vom Reader **nur** als `SparqlClient`-Trait/`Language` importiert — intern
für die Versions-/Manifestations-Auflösung im `AknFetcher` (JLX-TMP-02 via Bridge). **Kein
einziges JOLux-Metadaten-Primitiv ist als Tool projiziert.**

---

## 3. Die Lücken (Soll vs. Ist)

### G-1 — Die gesamte JOLux-Metadaten-Schicht fehlt als Tools  · *grösster Befund*

Von ~22 JOLux-Primitiven ist **keines** ein MCP-Tool. Das LLM kann über die MCP-Oberfläche
heute **nicht**:

| Modul | Primitive (Beispiele) | Was dem Agenten fehlt |
| --- | --- | --- |
| `search` / `resolve` | `search_law`, `resolve_sr_number`, `resolve_manifestation`, `list_expressions` | **Einstieg ohne ELI** (Discovery, vgl. ADR-006) |
| `temporal` | `check_in_force`, `list_versions`, `resolve_consolidation_at` | „Ist die Norm am Stichtag X in Kraft?", Fassungsliste |
| `impacts` | `get_impacts`, `get_outgoing_impacts`, `get_article_history` | „Was ändert diesen Erlass / was ändert er?" (Änderungsnetz) |
| `citations` | `get_citations` (in/out) | Zitationsnetz zwischen Erlassen |
| `taxonomy` | `get_taxonomy`, `find_related_by_topic` | Rechtsgebiet, thematisch verwandte Erlasse |
| `treaties` | `find_treaties`, `get_treaty_info` | Staatsverträge |
| `subdivisions` | `get_subdivisions`, `list_annexes` | Anhänge, Gliederung auf Metadaten-Ebene |
| `genesis` / `publication` | `get_drafts`, `get_consultations`, `get_oc_act`, `get_memorial`, `get_fga_documents` | Entstehungsgeschichte, amtliche Publikation |
| `vocabulary` | `list_vocabulary`, `explore_node`, `resolve_vocabulary_label` | kontrolliertes Vokabular |

Davon ist die **Discovery-Teilmenge** (`search_law`, `resolve_sr_number`, `find_related_by_topic`)
bereits in `40_FINDINGS.md`/ADR-006/M11 erfasst. **Neu hier:** Auch die **belegfähigen** (norm-
provenance-tragenden) Metadaten-Primitive — Inkraftsein, Versionen, Impacts, Zitate — sind nicht
projiziert. Das ist für ein juristisches Gutachten-System gravierend: „in Kraft am Stichtag?" und
„welche Norm ändert diese?" sind Kernfragen, die der Server beantworten *kann*, aber nicht
*anbietet*.

### G-2 — AKN: Aufbereitungs-Primitive  · *nutzwertige Lücken geschlossen (2026-06-21)*

Ausgangslage waren fünf nicht projizierte Primitive: `list_components` / `get_component_document`,
`extract_tables`, `detect_foreign_content`, `hollow_document`, `chunk_document`. Bewertung und
heutiger Stand:

- `extract_tables`, `list_components`, `detect_foreign_content` waren **nutzwertige Lücken**
  (Tabellen/Anhänge/fremdsprachige Blöcke kommen in echten Erlassen vor). **Geschlossen:** Die
  drei sind nun als gleichnamige MCP-Tools im Pool `LocalNavigation` projiziert
  (`mcp-reader/src/tools.rs`), in der Matrix als `Projected` verbucht (AKN-CMP-01, AKN-SPC-01,
  AKN-SPC-02) und durch Offline-Tests abgedeckt. `get_component_document` (AKN-CMP-02) bleibt
  bewusst `Excluded` — interner Helper, von `list_components` abgedeckt.
- `hollow_document`/`chunk_document` sind eher **RAG-Bausteine** für `semantic-fedlex` als
  direkte Agenten-Tools — **bewusster Ausschluss** (`CHK-*` als `Excluded` in der Matrix).


### G-3 — ansV-Blockade (unverändert gültig, jetzt breiter)

Die in `40_FINDINGS.md` beschriebene Blockade (kein Einstiegs-ELI) bleibt der akuteste Punkt.
G-1 zeigt aber: Selbst **mit** Discovery fehlen dem Agenten die belegfähigen Metadaten-Tools,
um „in Kraft?"/„geändert durch?" sauber zu beantworten.

### G-4 — Tool-Vollständigkeit nirgends als Test verankert  · *geschlossen (2026-06-16)*

~~Es gibt keinen Test „jedes im Lexikon dokumentierte, agenten-taugliche Primitiv hat ein Tool
**oder** einen begründeten Ausschluss".~~ **Geschlossen** durch
`crates/mcp-reader/tests/lexicon_projection.rs` (offline, läuft in jeder `cargo test`-Runde):
Die Vollständigkeits-Matrix ordnet jede der 47 Lexikon-IDs genau einem Zustand zu
(21 `Projected` / 26 `Excluded` mit Begründung); registrierte Composite-Tools ohne
Lexikon-Primitiv (aktuell nur `compare_versions`) stehen in der `COMPOSITE_TOOLS`-Allowlist.
Der Test wird rot, sobald ein neues Lexikon-Primitiv ohne Zuordnung oder ein nicht verbuchtes
Tool dazukommt — Lücken wie G-1/G-2 können so nicht mehr unbemerkt entstehen. Zugleich sind die
G-2-Primitive nun **explizit** als `Excluded` verbucht (Nutzwert-Lücke, bewusst zurückgestellt;
`CHK-*` als dauerhafter RAG-Ausschluss).

### G-5 — Live-Konformanz: Frequenz & Alarm  · *geschlossen (2026-06-21)*

~~Siehe §0. Wöchentlich ist für Drift-Erkennung grob; ein roter Lauf erzeugt heute kein
Issue/keine Benachrichtigung.~~ **Geschlossen** in `live-conformance.yml`:

- **Frequenz** von wöchentlich auf **zweimal/Woche** (Mo + Do, `cron: "17 5 * * 1,4"`) erhöht —
  endpunkt-schonend, aber engmaschigere Drift-Erkennung.
- **Alarm**: ein neuer `notify`-Job (`if: always()`, `actions/github-script`) öffnet bei rotem
  Lauf ein **dedupliziertes** GitHub-Issue (Label `live-conformance`) mit Triage-Reihenfolge
  („zuerst den Bund verdächtigen"); existiert bereits ein offenes Issue, wird nur kommentiert
  (kein Spam bei mehrtägiger Bund-Störung). Ein **grüner Folgelauf schliesst das Issue
  automatisch** wieder.


---

## 4. Reifegrad-Tabelle (korrigiert)

| Dimension | Stand | Beleg |
| --- | --- | --- |
| Architektur-Kern (M0–M9) | **stark** | Provenance-Gate, Tenant-Isolation, Quota, Outbox/DLQ — grün |
| Bi-temporaler Korpus | **stark** | Lücke B grün |
| Daten-/Kompositionsschicht | **stark & live-getestet** | jolux 29 + akn 12 + bridge 3 Live-Tests, wöchentlich in CI |
| Live-Konformanz in CI | **vorhanden** | `live-conformance.yml` (war im Roadmap-Entwurf falsch als „fehlt") |
| **MCP-Tool-Oberfläche** | **deutlich unvollständig** | nur 9 AKN-Tools; **0 von ~22 JOLux-Primitiven** + 5 AKN-Primitive nicht projiziert |
| ansV-Nutzbarkeit | **blockiert** | kein Einstiegs-ELI; zudem keine „in Kraft?/Impacts"-Tools |
| Betriebshärtung (M10) | **teilweise** | Health/Server/Cache/K8s erledigt; mTLS, Backup/Restore, Schema-Versionierung offen |

**Kernsatz.** Das Projekt hat eine **exzellente, breit getestete Datenschicht** und einen
**reifen Server-Kern**, aber die **MCP-Tool-Projektion ist die eigentliche Baustelle**: Sie
deckt heute nur den AKN-Lesepfad ab, während die komplette JOLux-Metadaten-Schicht (inkl. der
gutachtenkritischen Funktionen „in Kraft?", „Impacts", „Zitate", „Versionen") nicht als Tools
verfügbar ist.

---

## 5. Konsequenz für den Fahrplan

Diese Analyse weitet M11 von „nur Discovery" zu einer **systematischen Tool-Projektion** und
ersetzt den irrigen CI-Schritt. Details im aktualisierten `50_ROADMAP_TO_PERFECT.md`.

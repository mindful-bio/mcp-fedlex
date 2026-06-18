# 50 — Fahrplan zum „perfekten" MCP-Server

> **Was dieses Dokument ist.** Eine priorisierte Roadmap auf Basis der am Code verifizierten
> [45_GAP_ANALYSIS.md](45_GAP_ANALYSIS.md). Es trifft keine neuen Architekturentscheidungen (die
> stehen in den ADRs), sondern ordnet die **belegten** offenen Punkte nach Hebelwirkung und
> definiert je Schritt eine **Abnahme (Test-Nachweis)**. Stand: 2026-06-16 (überarbeitet nach
> Lücken-Analyse).

---

## Was „perfekt" hier heisst (Definition statt Wunschliste)

Ein MCP-Server ist im Sinne dieses Projekts „perfekt", wenn er **vier** Eigenschaften
gleichzeitig erfüllt — nicht, wenn er die meisten Tools hat:

1. **Vollständig.** Jedes agenten-taugliche Primitiv des Funktionsraums (`10_LEXICON_jolux.md`,
   `11_LEXICON_akn.md`) ist als MCP-Tool projiziert **oder** explizit begründet ausgeschlossen.
2. **Beweisbar.** Jede Behauptung hat einen grünen Test — Unit für Logik, **live** gegen Fedlex
   für die Daten-Konformanz. *(Hier bereits stark: jolux 29 + akn 12 + bridge 3 Live-Tests
   laufen wöchentlich in CI — siehe Korrektur unten.)*
3. **Nutzbar.** Der reale Konsument `ansV` kann den Normalfall (Frage ohne ELI) Ende-zu-Ende bis
   zum verifizierten Gutachten durchspielen.
4. **Betriebsfähig.** Die Day-2-Garantien (mTLS, Backup/Restore, Schema-Evolution) sind als
   nachweisbare Eigenschaft vorhanden.

---

## Reifegrad heute (verifiziert, siehe 45_GAP_ANALYSIS.md)

| Dimension | Stand |
| --- | --- |
| Architektur-Kern (M0–M9), bi-temporaler Korpus | **stark, grün** |
| Daten-/Kompositionsschicht (jolux/akn/bridge) | **stark & live-getestet** |
| Live-Konformanz in CI | **vorhanden** (`live-conformance.yml`, wöchentlich) |
| **MCP-Tool-Oberfläche** | **deutlich unvollständig**: nur 9 AKN-Tools; **0 von ~22 JOLux-Primitiven** projiziert |
| ansV-Nutzbarkeit | **blockiert** am Einstieg + fehlende „in Kraft?/Impacts"-Tools |
| Betriebshärtung (M10) | **teilweise** |

> **Korrektur gegenüber dem ersten Entwurf.** Die Annahme „Live-Konformanz läuft nie in CI" war
> falsch — der Workflow existiert. Der eigentliche, viel grössere Befund ist die **Tool-
> Projektion**: Die gesamte JOLux-Metadaten-Schicht ist nicht als MCP-Tools verfügbar (G-1).

---

## Fahrplan (priorisiert nach Hebelwirkung)

### Schritt 1 — M11: Discovery-Tools & Hinweis-Provenance  · *höchster Hebel, akut*

**Warum zuerst.** Ohne Einstiegs-ELI scheitert die ganze ansV-Kette am ersten Schritt (G-3).

**Inhalt.** Wie in [ADR-006](adr/ADR-006-discovery-tools-and-hint-provenance.md) /
[30_PLAN.md → M11](30_PLAN.md): `search_law`, `resolve_sr_number`, `find_related_topic`;
typisierte Hinweis-Provenance; `Discovery`-RBAC-Pool; Quota-Gewicht.

**Abnahme.** `tools/list` rollenabhängig; `kind: "hint"` im Wire-Format; Discovery→Beleg-Kette;
Quota-Test.

### Schritt 2 — JOLux-Metadaten-Tools projizieren  · *grösster struktureller Hebel (G-1)*

**Warum.** „Ist die Norm am Stichtag in Kraft?", „Welche Norm ändert diese?", „Welche Fassungen
gibt es?", „Wer zitiert wen?" sind **gutachtenkritische** Fragen. Die Primitive existieren und
sind live-getestet — es fehlt nur die Tool-Schicht. Diese Tools sind **norm-provenance-tragend**
(im Gegensatz zu Discovery), passen also direkt in das ADR-004-Gate.

**Inhalt (vorgeschlagene Tool-Tranchen, je dünner `McpTool`-Wrapper + Registrierung + Schema):**
- **Tranche A — Temporal (höchste Priorität): ✅ erledigt** — `check_in_force`, `list_versions`,
  `resolve_consolidation_at` in `crates/mcp-reader/src/metadata.rs` als `ToolPool::JoluxMetadata`,
  via `register_metadata_tools` registriert; Abnahme testgrün (siehe ADR-007).

- **Tranche B — Beziehungen: ✅ erledigt** — `get_impacts`, `get_outgoing_impacts`,
  `get_article_history`, `get_citations` in `crates/mcp-reader/src/metadata.rs` als
  `ToolPool::JoluxMetadata`, via `register_metadata_tools` registriert; Abnahme testgrün
  (Norm-Provenance, eID-Normalisierung J18.2, `(from,to)`-Dedup J7.4, `direction`; siehe ADR-007).

- **Tranche C — Einordnung: ✅ erledigt** — `get_taxonomy`, `get_subdivisions`, `list_annexes`
  in `crates/mcp-reader/src/metadata.rs` als `ToolPool::JoluxMetadata`, via
  `register_metadata_tools` registriert; Abnahme testgrün (Norm-Provenance, Sprach-Filter,
  optionaler `type_uri`-Filter, Annex-Spezialfall JLX-SUB-02; siehe ADR-007). Damit ist G-1
  (Projektion der JOLux-Metadaten-Schicht) geschlossen.

- **Tranche D — Kontext (optional/nachrangig):** `treaties`, `genesis`, `publication`,
  `vocabulary` — nur projizieren, wenn ein ansV-Bedarf belegt ist (sonst bewusst zurückstellen).

**Vorab geklärt — [ADR-007](adr/ADR-007-jolux-metadata-tools-pool-and-quota.md):** eigener Pool
`ToolPool::JoluxMetadata` (getrennt von `LocalNavigation`/`Discovery`), Quota-Gewicht **gleich
Discovery** (gleiches Live-Lastprofil), und **Norm-Provenance** für alle Tranchen inkl.
`get_taxonomy` (liest eine Eigenschaft eines *bekannten* Erlasses → norm; die Primitive setzen
`Provenance::new(...)` bereits selbst).


**Abnahme.** Pro Tool: `tools/list` zeigt es rollenabhängig; Dispatch-Test mit Norm-Provenance
im Wire-Format; mindestens ein Live-/Mock-Test, der das Primitiv über das Tool durchreicht.

### Schritt 3 — Tool-Vollständigkeit als Test verankern  · ✅ *erledigt — schliesst G-4*

**Warum.** Damit eine Lücke wie G-1 nie wieder unbemerkt entsteht.

**Inhalt.** Ein Test/Doku-Artefakt (Vollständigkeits-Matrix), das jedes im Lexikon
dokumentierte, agenten-taugliche Primitiv genau einem Zustand zuordnet: **projiziert** (Tool X)
oder **begründet ausgeschlossen** (z.B. `hollow_document`/`chunk_document` als RAG-Bausteine für
semantic-fedlex, G-2). Bricht, wenn ein neues Primitiv ohne Zuordnung dazukommt.

**Erledigt (2026-06-16).** Verankert in `crates/mcp-reader/tests/lexicon_projection.rs` (offline,
läuft in jeder `cargo test`-Runde, **kein** `#[ignore]`). Die `MATRIX` ordnet alle 47 Lexikon-IDs
zu (**21 projiziert / 26 begründet ausgeschlossen**); registrierte Composite-Tools ohne
Lexikon-Primitiv (aktuell nur `compare_versions`, Pool `Validation`) stehen in der
`COMPOSITE_TOOLS`-Allowlist ⇒ **22 registrierte MCP-Tools** insgesamt. Sechs Assertions:
Lexikon↔Matrix deckungsgleich (G-4-Schutz), jedes `Projected` real registriert, kein
registriertes Tool verwaist (Matrix ∪ Composite == Registry), Zähl-Invariante (21/26/+1/=22),
Pool-Zuordnung maschinell verifiziert (`pool_of`; Lexikon-Tools nie in `Validation`/`Workspace`,
`compare_versions` == `Validation`) sowie RBAC/Monotonie (Reader ⊆ Navigator ⊆ Validator;
Composite nur für Validator sichtbar).

**Abnahme.** ✅ Matrix lückenlos; CI-Test rot, wenn ein Lexikon-Primitiv ohne Tool/Ausschluss
auftaucht (`cargo test -p mcp-reader --test lexicon_projection`).

### Schritt 4 — End-to-End-Beleg-Kette ansV ↔ Reader  · *beweist „nutzbar"*

**Warum.** Schritte 1–2 machen Tools verfügbar; dieser Schritt beweist den ganzen Fluss.

**Inhalt.** Integrationstest (in `ansV`): Frage ohne ELI → `search_law`/`resolve_sr_number`
(Hinweis) → `check_in_force`/`read_article` (Norm) → zitierte Quelle endet `Verified`; geratener
ELI bleibt korrekt `Unverified` (Negativfall).

**Erledigt (2026-06-16).** Verankert in `crates/ansv-fedlex/tests/e2e_belegkette.rs` (offline,
läuft in jeder `cargo test`-Runde, **kein** `#[ignore]`). Ein winziger axum-Mock bedient beide
HTTP-Gegenstellen des `GutachtenRunner` — `POST /rpc` (Reader: `tools/list`/`tools/call` in der
echten `{data, provenance}`-Form) und `POST /chat/completions` (scriptgesteuertes LLM) —, sodass
der **ganze Lauf** deterministisch durchläuft. Test 1 (`frage_ohne_eli_endet_verifiziert`):
`search_law` (Hinweis, kein `eid`) → `read_article` (Norm) → die zitierte Quelle endet `Verified`
mit `text_match: true`, **belegt durch den `read_article`-Record** (nicht den Hinweis); Tool-Calls
stehen in erwarteter Reihenfolge im Evidence-Log **und** in den `RunEvent`s (+ `Done`). Test 2
(`geratener_eli_bleibt_unverifiziert`): ein neben Art. 19 erfundener Fremd-ELI bleibt korrekt
`Unverified`, während die echte Quelle `Verified` ist.

**Abnahme.** ✅ Beide Pfade grün; Evidence-Log enthält die erwarteten Tool-Calls in Reihenfolge
(`cargo test -p ansv-fedlex --test e2e_belegkette`).


### Schritt 5 — M10 abschliessen (Day-2-Härtung)  · *Betriebsfähigkeit*

**Inhalt.** mTLS/Zero-Trust intern (ADR-005); Oxigraph-Backup/Restore (B-1); AKN/JOLux-Schema-
Versionierung (B-2). Zusätzlich die kleine CI-Härtung aus G-5 (Alert/Issue bei rotem Live-Lauf,
ggf. häufigere Frequenz).

**Abnahme.** Je Punkt der in [30_PLAN.md → M10](30_PLAN.md) genannte Test-/Infra-Nachweis;
ehrliche Kennzeichnung Infra vs. cargo-test.

### Schritt 6 — AKN-Aufbereitungs-Tools (Rest) abwägen  · *Feinschliff (G-2)*

**Inhalt.** Entscheiden und festhalten: `extract_tables`, `list_components`,
`detect_foreign_content` als Tools projizieren (nutzwertig für echte Erlasse) vs.
`hollow_document`/`chunk_document` bewusst ausschliessen (RAG-Schicht). Ergebnis fliesst in die
Matrix aus Schritt 3.

---

## Reihenfolge-Begründung in einem Satz

Erst den **akuten Engpass** öffnen (1), dann die **strukturelle Hauptlücke** der Tool-Projektion
schliessen (2) und gegen Rückfall **absichern** (3), dann die **Nutzbarkeit** Ende-zu-Ende
beweisen (4), zuletzt **betrieblich härten** (5) und den AKN-**Feinschliff** abwägen (6).

## Nicht-Ziele (bewusst nicht auf dem Fahrplan)

- **Tools „auf Verdacht".** Der Funktionsraum ist die Obergrenze; Tranche D nur bei belegtem
  ansV-Bedarf.
- **Norm-Provenance an Suchtreffer.** Verworfen in ADR-006 (verwischt *gefunden* vs. *belegt*).
- **CI „nightly Live-Konformanz" neu bauen.** Existiert bereits (wöchentlich); nur Härtung nötig.

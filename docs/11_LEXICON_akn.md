# Capability-Lexikon AKN — der vollständige Funktionsraum

> **Was dieses Dokument ist.** Die systematische, schema-getriebene Enumeration **aller** Operationen, die das Akoma-Ntoso-3.0-Datenmodell von Fedlex hergibt. Abgeleitet aus dem AKN-3.0-Standard (OASIS), gefiltert durch die empirische Realität des Fedlex-Corpus (147 genutzte Tags, 5 Dokumentmuster, verifiziert auf 15'807 XML-Dateien, 0 Parse-Fehler). Es ist **kein** Tool-Katalog eines bestimmten Servers, sondern das Vokabular, aus dem Konsumenten komponieren.
>
> **Wer es konsumiert.**
> - `mcp-fedlex` implementiert Primitive als MCP-Tools (Projektion, nicht Quelle).
> - `skills-fedlex` komponiert Primitive zu juristischen Playbooks.
> - `semantic-fedlex` nutzt die Destillations-Primitive (CHK) als Ingest-Spezifikation.
> - `syllogismus-fedlex` referenziert Primitive in auditierbaren Schluss-Schritten.
> - Orchestratoren (z.B. OpenClaw) planen über Lexikon-IDs statt über Tool-Namen.
>
> **Quellen (Ground Truth).**
> - Empirisches Rulebook X0–X20: `fedlex-RAG-evaluation/data_understanding/xml_akn/rulebook_xml_akn.md`
> - Implementierungs-Spec: `../../analyse-fedlex/11_DATA_RULES_akn.md`
> - PoC-Tools 13–17: `../../analyse-fedlex/30_TOOL_CATALOG.md` (`akn_tools.py`, `akn_middleware.py`)
> - Referenz-Implementierung: `crates/fedlex-akn` (alle 20 Primitive, Konformanz-Suite grün)
>
> **Pendant.** Der JOLux-Funktionsraum (Metadaten/Graph) steht in `10_LEXICON_jolux.md`. AKN liefert den **Text**, JOLux die **Metadaten** (X0.3). Die Brücke ist die FRBR-Manifestation-URL (JLX-RES-04 → AKN-DOC-01).

---

## Inhalt

- [Methodik und Vollständigkeitsbeweis](#methodik-und-vollständigkeitsbeweis)
- [Eintragsformat](#eintragsformat)
- [Domäne 1 — Dokument & Identität (DOC)](#domäne-1--dokument--identität-doc)
- [Domäne 2 — Struktur-Navigation (STR)](#domäne-2--struktur-navigation-str)
- [Domäne 3 — Text-Extraktion (TXT)](#domäne-3--text-extraktion-txt)
- [Domäne 4 — Änderungsmarkup (MOD)](#domäne-4--änderungsmarkup-mod)
- [Domäne 5 — Referenzen (REF)](#domäne-5--referenzen-ref)
- [Domäne 6 — Components & Anhänge (CMP)](#domäne-6--components--anhänge-cmp)
- [Domäne 7 — Tabellen & Fremdinhalte (SPC)](#domäne-7--tabellen--fremdinhalte-spc)
- [Domäne 8 — Destillation & Chunking (CHK)](#domäne-8--destillation--chunking-chk)
- [Vollständigkeits-Matrix](#vollständigkeits-matrix)
- [Explizit ausgeschlossene Elemente](#explizit-ausgeschlossene-elemente)
- [Kompositions-Karte](#kompositions-karte)
- [Statistik](#statistik)
- [Konformanz-Suite](#konformanz-suite)

---

## Methodik und Vollständigkeitsbeweis

Vollständigkeit wird hier **konstruktiv** behauptet und ist nachprüfbar:

1. **Soll:** Jedes der 147 im Corpus vorkommenden Tags (66 AKN-Standard + 81 fremd/proprietär, X15.2), jede genutzte Meta-Sektion und jedes der 5 Dokumentmuster wird genau einem Lexikon-Eintrag zugeordnet — oder explizit ausgeschlossen (mit empirischer Begründung, siehe [Ausschlussliste](#explizit-ausgeschlossene-elemente)).
2. **Ist:** Jeder Eintrag trägt die empirische Volumetrie aus dem Rulebook (X-Regel-Referenz). Primitive über nie genutzte Elemente existieren nicht (17 von 28 Hierarchie-Elementen, 9 von 11 Meta-Sektionen, 11 von 12 Dokumenttypen sind tot, X17).
3. **Zugriffsmuster:** Pro Dokument werden vier Muster geprüft. *extract* (Daten herauslesen), *navigate* (per eId/Pfad bewegen), *enumerate* (Elemente auflisten/filtern), *transform* (verlustarm umformen — Hollowing, Markdown, Chunks). Wo Muster zusammenfallen, ist das im Eintrag vermerkt.

Die [Vollständigkeits-Matrix](#vollständigkeits-matrix) am Ende ist der Audit-Trail.

**Architektonischer Unterschied zu JOLux.** JOLux-Primitive sind Queries gegen einen Live-Endpoint. AKN-Primitive sind **Funktionen über einem Dokument** — sie operieren auf einer per AKN-DOC-01 beschafften, geparsten XML-Datei. Der Stichtag wird **vor** dem Fetch aufgelöst (JLX-TMP-02 → JLX-RES-04), danach ist das Dokument immutabel.

## Eintragsformat

```
### AKN-<DOM>-<NN> · <name>
Frage        die juristische/agentische Frage, die das Primitiv beantwortet
Signatur     (input) → output   — konzeptionell, nicht wire-format
AKN          beteiligte Elemente & Attribute
Empirie      Volumetrie, Verifikationsreferenz (X-Regel)
Falltraps    bekannte Fallen (Muster, Duplikate, XSD-Lücken)
Komposition  typische Vorgänger (←) und Nachfolger (→)
Status       PoC-validiert | skript-verifiziert | abgeleitet (neu) | …
```

Querschnitt-Invarianten (gelten für **jedes** Primitiv, werden nicht wiederholt):

- **Provenance-Pflicht.** Jede Antwort trägt `{ eli, valid_as_of, transaction_time }` (ADR-004). `valid_as_of` ist der Stichtag, mit dem die Manifestation aufgelöst wurde.
- **Parse-Toleranz.** Fedlex-XML ist **nicht strikt XSD-valid** (5 Violations, X17.1). Nie gegen das XSD validieren — 8.1 % der Body-Kinder wären sonst Fehler.
- **Text-Normalisierung.** Soft-Hyphens (`\xad`, 3'131×) entfernen, NBSP (`\xa0`, 662'695×) zu Leerzeichen (X18.3). Sonst scheitert exaktes Text-Matching.
- **eId-Normalisierung** vor jedem AKN↔JOLux-Abgleich. `_([a-z])($|/)` → `$1$2` (X9.4). In `fedlex-jolux` als `normalize_eid` implementiert.
- **Inline-Flattening.** `<b>/<i>/<sup>/<sub>/<span>/<br>` zu Plaintext, `<inline name="man-*">`-Wrapper verwerfen (reine Word-Konvertierungs-Artefakte, X18.5).
- **Graceful Failure.** `{ error, hint }`, nie Crash.

---

## Domäne 1 — Dokument & Identität (DOC)

Der Einstieg. Beschaffung, FRBR-Identität, Muster-Klassifikation. Alles Weitere setzt ein geparstes Dokument voraus.

### AKN-DOC-01 · fetch_akn_document
- **Frage:** "Gib mir das maschinenlesbare Dokument zu ELI X am Stichtag Y in Sprache Z"
- **Signatur:** `(eli, as_of?, lang?) → ParsedDocument`
- **AKN:** Wurzel `<akomaNtoso>` → exakt ein `<act name="publicLaw">` (100 %, X1.1). Beschaffung über die FRBR-Manifestation-URL aus JLX-RES-04.
- **Empirie:** 15'807 Dateien, 0 Parse-Fehler (X15.1). Encoding sauber (0 Replacement-Zeichen, X18.3).
- **Falltraps:** XML-Manifestationen existieren erst ab ~2021 — ältere Fassungen nur PDF-A (J14.2). 34 % der Dateien sind body-lose Metadaten-Stubs (X1.2) — vor der Textverarbeitung AKN-DOC-03 fragen. Live liefert Fedlex alle 3+ Sprachen, der Analyse-Bulk-Download war DE-monolingual (X0.3b) — die Empirie-Zahlen dieses Lexikons beziehen sich auf DE.
- **Komposition:** ← JLX-TMP-02 (Stichtagsfassung), JLX-RES-04 (URL) | → alle anderen AKN-Primitive
- **Status:** **produktiv implementiert** (`fedlex-bridge::AknFetcher::fetch_akn_document` — Komposition JLX-TMP-02 → Download → `AknDocument::parse`, Cache pro Manifestations-URL) + konformanz-getestet (`bridge_conformance.rs` live, 4 Offline-Tests mit Mocks)

### AKN-DOC-02 · get_frbr_metadata
- **Frage:** "Welches Werk, welche Fassung, welche Sprache ist diese Datei?" (Selbstauskunft)
- **Signatur:** `(doc) → { eli_work, eli_expression, eli_manifestation, sr_number, title, language, dates{}, author }`
- **AKN:** `<meta>/<identification>` mit WEMI-Kette `FRBRWork → FRBRExpression → FRBRManifestation` (X19.1). Felder: `FRBRuri`, `FRBRnumber`, `FRBRname`, `FRBRlanguage`, `FRBRdate[@name]`, `FRBRauthor` → `<TLCOrganization @showAs>`.
- **Empirie:** 7 Kernfelder in 100 % der Dateien (X2.2). `FRBRdate`-Namen sind jolux-spezifisch (`jolux:dateDocument` 100 %, `jolux:publicationDate` 72.3 %, `jolux:dateEntryInForce` 52.8 %, `jolux:dateApplicability` nur CC, X19.4).
- **Falltraps:** **17.1 % der Dateien tragen mehrere `<FRBRExpression>`-Blöcke** (bis 36, X17.6) — der Parser muss n≥1 handhaben. `FRBRauthor`/`FRBRdate` sind auf Work und Expression identisch gespiegelt — einmal lesen genügt (X19.3). `FRBRitem` existiert nie. `FRBRcountry` ist immer `ch`.
- **Komposition:** ← AKN-DOC-01 | → Konsistenz-Check gegen JLX-RES-03 (dieselben Felder aus dem Graphen)
- **Status:** implementiert + konformanz-getestet (`get_frbr_metadata`, `akn_doc_02_frbr_metadata_eng`)

### AKN-DOC-03 · classify_pattern
- **Frage:** "Was für eine Art Dokument ist das — und welche Verarbeitungsstrategie braucht es?"
- **Signatur:** `(doc) → { pattern: Structured|FlatArticles|LevelBased|NoBody|Amendment|Other, has_body, article_count, component_count }`
- **AKN:** Entscheidung über Body-Präsenz und dominante Kind-Typen (`<article>`, `<level>`, `<mod>`).
- **Empirie:** 5+1 Muster (X4.2). LEVEL_BASED 34.9 %, NO_BODY 34.0 %, FLAT_ARTICLES 23.5 %, AMENDMENT 3.4 %, STRUCTURED 2.8 %, OTHER 1.4 %. CC = 74 % FLAT_ARTICLES, OC = 68.3 % LEVEL_BASED, FGA = 50.7 % NO_BODY (X13).
- **Falltraps:** **91 % der OC/FGA-Dateien haben keine Artikel** (X5.2) — wer blind Artikel-Primitive aufruft, bekommt leere Antworten und hält sie für Fehler. NO_BODY-Dateien sind historische Stubs (Staatsverträge vor 1960), für inhaltliche Fragen ignorierbar (X8.3). `<mainBody>` erscheint NIE auf act-Ebene (X4.1).
- **Komposition:** ← AKN-DOC-01 | Strategie-Weiche für STR/TXT/MOD/CHK
- **Status:** implementiert + konformanz-getestet (`classify_pattern`, `akn_doc_03_pattern_eng`)

---

## Domäne 2 — Struktur-Navigation (STR)

Die Vollstruktur, die JOLux nicht hat (dort max. 8.5 % Abdeckung, J4.1). Hier ist sie zu 100 % vorhanden.

### AKN-STR-01 · get_document_structure
- **Frage:** "Wie ist dieses Gesetz gegliedert?" (Inhaltsverzeichnis)
- **Signatur:** `(doc, type_filter?, max_depth?) → [{ eid, type, num, heading, depth, children[] }]`
- **AKN:** Die 11 real genutzten Hierarchie-Elemente. `paragraph` (170'214), `article` (70'009), `level` (44'187), `section`, `chapter`, `title`, `subdivision`, `part`, `transitional`, `proviso`, `book` (1×, nur ZGB) (X17.3). Tiefste Kette `book > part > title > chapter > section > article > paragraph`.
- **Empirie:** 597'278 eIds, konsistentes Präfix-Schema (`art_`, `chap_`, `sec_`, `annex_`, `lvl_`, X9.2). Verschachtelungstiefe 5–24 (X5.3).
- **Falltraps:** Bei LEVEL_BASED-Dokumenten (35 %) heisst die Gliederung `<level>` mit generischen eIds (`lvl_1/lvl_2`) — Überschriften statt Typen tragen die Semantik. Mit `type_filter=article` ist dies zugleich die Artikel-Enumeration.
- **Komposition:** ← AKN-DOC-01/03 | → AKN-TXT-01/02 (Text pro Element), JLX-SUB-01 (Abgleich der Impact-Ziele)
- **Status:** implementiert + konformanz-getestet (`get_document_structure`, `akn_str_structure_resolve_and_path_eng`)

### AKN-STR-02 · resolve_eid
- **Frage:** "Gib mir das Element `art_14a`" (Punkt-Lookup)
- **Signatur:** `(doc, eid) → { element, duplicates_found }`
- **AKN:** `@eId`-Attribut, Pfad-Notation mit `/` (`art_14/para_1/lbl_a`, X9.3).
- **Empirie:** 842 von 15'806 Dateien (~5.3 %) haben **doppelte eIds** (9'304 Duplikate, v.a. Anhänge alter Gesetze, X15.3).
- **Falltraps:** **Eindeutigkeit ist NICHT garantiert** — der Lookup muss Duplikate abfangen und ausweisen. Vor JOLux-Abgleich normalisieren (`art_14_a` → `art_14a`, X9.4). eIds sind sprachinvariant (J13.2) — derselbe Lookup funktioniert in DE/FR/IT.
- **Komposition:** ← AKN-STR-01, JLX-IMP-02 (Impact-Ziel als eId) | → AKN-TXT-02
- **Status:** implementiert + konformanz-getestet (`resolve_eid`, `akn_str_structure_resolve_and_path_eng`)

### AKN-STR-03 · get_section_path
- **Frage:** "In welchem Kapitel/Abschnitt steht Art. 15?" (Kontext-Pfad)
- **Signatur:** `(doc, eid) → [{ eid, type, num, heading }]` (Wurzel → Element)
- **AKN:** Vorfahren-Kette über die eId-Pfad-Hierarchie und die DOM-Eltern.
- **Empirie:** Pflicht-Metadatum jedes Chunks (`section_path`, X14.3).
- **Falltraps:** eId-Pfade (`chap_1/sec_2`) und DOM-Verschachtelung stimmen fast immer überein, aber der DOM ist die Wahrheit — eIds älterer Dateien sind teils flach vergeben.
- **Komposition:** ← AKN-STR-02 | → AKN-CHK-02 (Kontext-Metadaten), Zitier-Anzeigen in syllogismus-fedlex
- **Status:** implementiert + konformanz-getestet (`get_section_path`, `akn_str_structure_resolve_and_path_eng`)

---

## Domäne 3 — Text-Extraktion (TXT)

Der eigentliche Gesetzestext. Existiert ausschliesslich hier — JOLux hat 0 % Textabdeckung (X0.3).

### AKN-TXT-01 · get_article_text
- **Frage:** "Was sagt Art. 15?" (der häufigste Einzelabruf überhaupt)
- **Signatur:** `(doc, eid, hollow?) → { num, heading, text, notes[], section_path }`
- **AKN:** `<article>` → `<num>`, `<heading>`, `<paragraph>` → `<content>` → `<p>`, Listen via `<blockList>/<listIntroduction>/<item>`.
- **Empirie:** 70'009 Artikel. Median ~550 Zeichen (~140 Tokens), 62.6 % der CC-Artikel zwischen 200 und 1'000 Zeichen (X10.2, GAP I4).
- **Falltraps:** `<authorialNote>` **vor** der Textextraktion aus dem Fluss separieren — sie ist Änderungshistorie, kein Normtext (X6.4). `<num>` enthält oft `<placeholder>` (96.5 % aller placeholder, X16.6). Hollowing beachten, wenn das Element Kinder hat (AKN-CHK-01).
- **Komposition:** ← AKN-STR-02 | ⊕ JLX-IMP-02 (Historie zum selben Artikel), → syllogismus-fedlex (Normtext-Zitat)
- **Status:** implementiert + konformanz-getestet (`get_article_text`, `akn_txt_article_text_and_notes_eng`)

### AKN-TXT-02 · get_element_text
- **Frage:** "Was steht in Level/Abschnitt/Anhang X?" (generische Variante für Nicht-Artikel)
- **Signatur:** `(doc, eid, hollow?) → { type, num?, heading?, text, notes[] }`
- **AKN:** alle 11 Hierarchie-Elemente, gleiche Extraktionsregeln wie TXT-01.
- **Empirie:** Notwendig für 66 % des Corpus — OC ist level-basiert, FGA component-basiert (X4.2). `<subdivision>` ist nie verschachtelt, Kinder nur `<heading>` + `<paragraph>` (X18.6).
- **Falltraps:** Wer nur TXT-01 implementiert, kann 91 % der OC/FGA-Dateien nicht lesen (X5.2).
- **Komposition:** ← AKN-STR-01/02 | → AKN-CHK-02
- **Status:** implementiert + konformanz-getestet (`get_element_text`, Offline-Test `element_text_works_for_levels`)

### AKN-TXT-03 · get_readable_document
- **Frage:** "Gib mir das ganze Gesetz lesbar" (Makro-Abruf für Mensch oder LLM-Kontext)
- **Signatur:** `(doc, include_notes?) → markdown`
- **AKN:** `<preface>` (Titel, SR-Nr.), `<preamble>` (Rechtsgrundlage, 99.9 % bei CC/OC), `<body>` komplett, `<signature>`-Blöcke am Ende (1'289×, X18.7).
- **Empirie:** Textvolumen pro Datei. CC Median 9'681 Zeichen, Max 425'018 (X10.1). Destillation = AKN → strukturerhaltendes Markdown.
- **Falltraps:** Naive Element-Konkatenation erzeugt **87 % Redundanz** — Hollowing (AKN-CHK-01) ist Pflicht-Vorstufe (X20). Tabellen als Markdown-Tabellen erhalten, nicht zeilenweise zerreissen (X6.3).
- **Komposition:** ← AKN-DOC-01 | das Makro-Tool der Thesis (`akn_middleware`)
- **Status:** implementiert + konformanz-getestet (`get_readable_document`, `akn_txt_03_readable_markdown_eng`)

### AKN-TXT-04 · search_text
- **Frage:** "Wo im Gesetz kommt 'Eigenverbrauch' vor?"
- **Signatur:** `(doc, query, max_hits?) → [{ eid, snippet, section_path }]`
- **AKN:** Volltextsuche über die gehollowten Blatt-Texte, Treffer auf eId-Ebene.
- **Empirie:** Mikro-Scope-Suche **innerhalb** eines Erlasses — komplementär zu semantic-fedlex (Cross-Law, Embedding-basiert).
- **Falltraps:** Ohne Text-Normalisierung (Soft-Hyphen, NBSP) entgehen Treffer (X18.3). Suche über Eltern-Elemente ohne Hollowing liefert Mehrfach-Treffer desselben Texts.
- **Komposition:** ← AKN-DOC-01, AKN-CHK-01 | → AKN-TXT-01 (Treffer-Artikel lesen)
- **Status:** implementiert + konformanz-getestet (`search_text`, `akn_txt_search_eng`)

---

## Domäne 4 — Änderungsmarkup (MOD)

Die Änderungshistorie **im Text** — komplementär zum JOLux-Impact-Graphen (Merge ist die eigentliche Capability).

### AKN-MOD-01 · get_modifications
- **Frage:** "Welche neuen Wortlaute verfügt dieser Änderungserlass?" (OC/AMENDMENT-Dokumente)
- **Signatur:** `(doc) → [{ mod_eid, quoted_root_type, quoted_eid, new_text }]`
- **AKN:** `<mod>` → `<quotedStructure>` mit dem neuen Wortlaut (X7.2).
- **Empirie:** 12'481 mods, 28.6 % der OC-Dateien (X7.1). **Jeder `<mod>` enthält exakt 1 `<quotedStructure>`** — die 1:1-Beziehung ist zu 100 % bestätigt (GAP I3).
- **Falltraps:** Die Wurzel im `<quotedStructure>` ist zu **95.6 % ein `<paragraph>`**, nicht ein `<article>` (X7.2) — geändert wird absatzweise. Für OC-RAG nicht den Rohtext chunken, sondern die quotedStructure-Blöcke extrahieren (X13.2).
- **Komposition:** ← AKN-DOC-03 (Muster AMENDMENT) | ⊕ JLX-IMP-03 (welche Zielgesetze), → Vorher/Nachher-Vergleich
- **Status:** implementiert + konformanz-getestet (`get_modifications`, `akn_mod_01_modifications_oc_stromgesetz`)

### AKN-MOD-02 · extract_change_notes
- **Frage:** "Wann und wodurch wurde dieser Absatz zuletzt geändert?" (Fussnoten-Historie)
- **Signatur:** `(doc, eid?) → [{ note_eid, anchor_eid, text, refs[{href, label}] }]`
- **AKN:** `<authorialNote marker="N">` mit Prosa-Historie und `<ref>` auf AS/OC-Einträge ("Fassung gemäss Ziff. I des BG vom …, in Kraft seit … (AS 2020 752)").
- **Empirie:** 77'349 Notes. **71.3 % enthalten ≥1 `<ref>`**, Median nur 30 Zeichen (X6.4, GAP I6).
- **Falltraps:** Das `placement`-Attribut fehlt zu 100 % (XSD-Violation V2) — nie darauf filtern. Notes sind die **textnahe** Änderungsquelle. Der JOLux-Impact-Graph (JLX-IMP-01/02) ist die **strukturierte** — beide divergieren, Merge nötig.
- **Komposition:** ← AKN-TXT-01 (Notes fallen dort an) | ⊕ JLX-IMP-02, → JLX-PUB-01 (AS-Ref auflösen)
- **Status:** implementiert + konformanz-getestet (`extract_change_notes`, `akn_txt_article_text_and_notes_eng`)

---

## Domäne 5 — Referenzen (REF)

Inline-Zitationen im Text. Artikel-genau — was der JOLux-Zitationsgraph (nur `/text`-Granularität) nicht kann.

### AKN-REF-01 · get_all_references
- **Frage:** "Auf welche Normen verweist dieses Gesetz im Text — und von wo?"
- **Signatur:** `(doc, context_eid?) → [{ source_eid, href?, label, target_kind }]`
- **AKN:** `<ref href="…">` (83'710×, X11.1).
- **Empirie:** 70.8 % absolute ELI-URIs (`https://fedlex.data.admin.ch/eli/…`), 7.6 % andere Fedlex-URIs (Vokabulare), 6.5 % extern, **15.0 % ohne href** (X11.2, GAP C4). Ref-Ziele zeigen zu **100 % auf Work-Level** — nie auf Sprachfassung oder Format (X19.6).
- **Falltraps:** Der Overlap mit dem JOLux-Zitationsgraphen beträgt nur **0–48 %** (J7.3) — keine Quelle ersetzt die andere, das vollständige Zitationsnetz ist der Merge mit JLX-CIT-01. `source_eid` (wo der Ref steht) liefert die Artikel-Granularität, die JOLux fehlt.
- **Komposition:** ← AKN-DOC-01 | ⊕ JLX-CIT-01 (Merge), → JLX-RES-03 (Ziel auflösen)
- **Status:** implementiert + konformanz-getestet (`get_all_references`, `akn_ref_01_references_eng`)

### AKN-REF-02 · parse_unlinked_refs
- **Frage:** "Worauf zeigen die 15 % Refs ohne href?"
- **Signatur:** `(ref_text, context) → { kind: Article|SrNumber|AsCitation|Unknown, parsed }`
- **AKN:** `<ref>` ohne `@href` — Zielangabe nur im Text ("Art. 5", "101", "AS 2020 752").
- **Empirie:** 12'490 Refs ohne href (XSD-Violation V1 — das XSD verlangt href als required, X17.1). OC 32.4 % leer, FGA 15.9 %, CC nur 0.4 %.
- **Falltraps:** Kontextabhängig — "101" in der Preamble ist die SR-Nummer der BV, "Art. 5" meint meist das eigene Gesetz. Heuristik, kein Determinismus — Konfidenz ausweisen.
- **Komposition:** ← AKN-REF-01 | → JLX-RES-01 (SR auflösen), AKN-STR-02 (interner Artikel)
- **Status:** implementiert + konformanz-getestet (`parse_unlinked_ref`, `akn_ref_01_references_eng` + Offline-Heuristik-Tests)

---

## Domäne 6 — Components & Anhänge (CMP)

Anhänge sind **eigenständige FRBR-Werke** innerhalb der Akte — nicht blosse Abschnitte.

### AKN-CMP-01 · list_components
- **Frage:** "Welche Anhänge/Beilagen enthält diese Datei?"
- **Signatur:** `(doc) → [{ component_index, doc_name, eli_work, title, is_empty_stub }]`
- **AKN:** `<components>` → `<component>` → `<doc name="annex">` (X8). Jedes Component-Doc hat eine **eigene `<identification>`** mit vollem FRBR (5'195/5'195, X19.8).
- **Empirie:** 5'195 Component-Docs in 2'700 `<components>`-Blöcken. Components existieren fast nur in Dateien **mit** Body — als Anhänge zu Gesetzen, nicht als Ersatz für body-lose Stubs (nur 2/5'378 NO_BODY-Dateien haben welche, GAP I2).
- **Falltraps:** JOLux-Annex-Subdivisions (JLX-SUB-02) und AKN-Components sind verschiedene Sichten — JOLux kennt nur Annexe mit Impacts (z.B. EnG: 0 in JOLux, Anhänge aber im XML). 267 Leer-Stubs (<100 Zeichen) überspringbar (X18.1).
- **Komposition:** ← AKN-DOC-01 | ⊕ JLX-SUB-02 (Abgleich), → AKN-CMP-02
- **Status:** implementiert + konformanz-getestet (`list_components`, `akn_cmp_components_eng`)

### AKN-CMP-02 · get_component_document
- **Frage:** "Gib mir Anhang 1 als eigenes Dokument"
- **Signatur:** `(doc, component_index) → ParsedDocument` (rekursiv alle AKN-Primitive anwendbar)
- **AKN:** `<component>/<doc>` mit **`<mainBody>`** (nie `<body>`!) — der einzige Ort, wo `<mainBody>` vorkommt (X4.1, GAP I1).
- **Empirie:** Median 2'212 Zeichen, Max 1.18 MB. Hierarchie wie act-Bodies (`<level>` 15'410, `<paragraph>` 11'077, `<article>` 3'139, X18.1).
- **Falltraps:** `<h1>` (HTML-Erweiterung, 180×) kommt nur hier vor (X17.9). Dieselben Hollowing-/Chunking-Regeln wie für act-Bodies anwenden.
- **Komposition:** ← AKN-CMP-01 | → AKN-STR/TXT/CHK rekursiv
- **Status:** implementiert + konformanz-getestet (`get_component_document`, `akn_cmp_components_eng`)

---

## Domäne 7 — Tabellen & Fremdinhalte (SPC)

Inhalte, die kein Fliesstext sind und Sonderbehandlung brauchen.

### AKN-SPC-01 · extract_tables
- **Frage:** "Welche Tabellen enthält das Dokument?" (Tarife, Grenzwerte, Listen)
- **Signatur:** `(doc, context_eid?) → [{ eid?, rows, cols, header[], data[][], oversized }]`
- **AKN:** `<table>/<tr>/<td>/<th>` (21'836 Tabellen, 981'890 Zellen, X6.3).
- **Empirie:** Tabellen-Präsenz CC 73.8 %, OC 90.9 % der Dateien. Median 1 Zeile, 75.3 % haben 1–5 Zeilen, 1.6 % haben 100+ Zeilen (Max 5'308, X16.4).
- **Falltraps:** Tabellen **als Einheit** behandeln — nie zeilenweise chunken (X6.3). Nur die 1.6 % Riesen-Tabellen splitten. 27 Zellen enthalten direkten Text statt `<p>` (Violation V5) — tolerant lesen.
- **Komposition:** ← AKN-TXT-02/03 (Tabellen fallen dort an) | → AKN-CHK-02 (Tabellen-Chunks)
- **Status:** implementiert + konformanz-getestet (`extract_tables`, `akn_spc_tables_and_foreign_eng`)

### AKN-SPC-02 · detect_foreign_content
- **Frage:** "Enthält das Dokument Formeln, Grafiken oder eingebettete Fremdformate?"
- **Signatur:** `(doc) → [{ eid_context, kind: Svg|MathMl|Skos|Ooxml, element_count }]`
- **AKN:** `<foreign>`-Container mit SVG (78, korrekter Namespace), MathML (~2'312 in 13 Dateien), SKOS (~1'818 in 18 Dateien), OOXML (468 in 15 Dateien) (X17.9, X18.4).
- **Empirie:** Nur 24 Dateien betroffen — selten, aber juristisch relevant (Berechnungsformeln in Verordnungen).
- **Falltraps:** **MathML-Elemente tragen keinen eigenen Namespace** — sie fallen unter den AKN-NS und müssen über lokale Elementnamen (`<mi>`, `<mo>`, `<mn>`, `<mrow>`) erkannt werden (X18.4). Für Text-RAG als Einheit markieren oder skippen, nie elementweise zerlegen.
- **Komposition:** ← AKN-DOC-01 | → Chunk-Annotation (CHK-02), ggf. separates Formel-Rendering
- **Status:** implementiert + konformanz-getestet (`detect_foreign_content`, `akn_spc_tables_and_foreign_eng`)

---

## Domäne 8 — Destillation & Chunking (CHK)

Die Transformations-Schicht. Macht AKN-Inhalte redundanzfrei und RAG-tauglich — die Ingest-Spezifikation für semantic-fedlex.

### AKN-CHK-01 · hollow_document
- **Frage:** "Gib mir jedes Element genau einmal mit seinem eigenen Text" (Redundanz-Eliminierung)
- **Signatur:** `(doc) → [{ eid, is_leaf, own_text | placeholder }]`
- **AKN:** Blatt-Test pro eId-Element. Blätter behalten Volltext, Eltern erhalten Platzhalter (`[Siehe Unterelemente: …]`, X20.2).
- **Empirie:** **87.1 % Redundanz** ohne Hollowing — EnG: 117'647 Zeichen roh → 15'156 Zeichen gehollowt. 655 von 779 Elementen sind Blätter (84.1 %) (X20.1, `_verify_hollowing.py`).
- **Falltraps:** `<authorialNote>` vor dem Blatt-Test aus dem Textfluss nehmen, sonst zählt Fussnotentext als Element-Text. Verschachtelte `blockList` (bis Tiefe 4, X16.3) als Einheit am Blatt belassen.
- **Komposition:** ← AKN-DOC-01 | Pflicht-Vorstufe für TXT-03, TXT-04 und CHK-02
- **Status:** implementiert + konformanz-getestet (`hollow_document`, `akn_chk_01_hollowing_eng`)

### AKN-CHK-02 · chunk_document
- **Frage:** "Zerlege das Dokument in retrieval-taugliche Einheiten mit Kontext"
- **Signatur:** `(doc, strategy?) → [{ chunk_id, text, metadata{ sr, title, eli, lang, date, section_path, eid, collection } }]`
- **AKN:** musterabhängige Strategie (X14.2). FLAT/STRUCTURED → Artikel, LEVEL_BASED → Level-Einheit, NO_BODY → Component-Doc, AMENDMENT → quotedStructure, OTHER → `<p>`-Gruppen.
- **Empirie:** Chunk-Median ~550 Zeichen (~140 Tokens, X10.2). 8 Pflicht-Metadaten pro Chunk (X14.3).
- **Falltraps:** **Chunking ist ein 5-Strategien-Problem** — 66 % der Dateien brauchen kein Artikel-Chunking (X14.4). Artikel >2'000 Zeichen am `<paragraph>` splitten. Tabellen als Einheit, Signature-Blöcke als eigene Einheit oder an den letzten Artikel (X18.7).
- **Komposition:** ← AKN-DOC-03 (Strategie), AKN-CHK-01 (Hollowing) | → semantic-fedlex Ingest, `mcp-ingest`
- **Status:** implementiert + konformanz-getestet (`chunk_document`, `akn_chk_02_chunks_eng`)

---

## Vollständigkeits-Matrix

Jede Element-Gruppe des Corpus → Primitive. (ex)tract, (n)avigate, (en)umerate, (t)ransform.

| # | Element-Gruppe | Vorkommen | Primitive | Muster |
|---|----------------|-----------|-----------|--------|
| 1 | `akomaNtoso`/`act` | 15'807 | DOC-01 | ex |
| 2 | `meta`/`identification`/`FRBR*` | 21'001 Blöcke | DOC-02 | ex |
| 3 | `references`/`TLCOrganization`/`TLCRole` | 15'809 | DOC-02 (Autor-Auflösung) | ex |
| 4 | `preface`/`docTitle`/`docNumber` | 20'716 | DOC-02, TXT-03 | ex |
| 5 | `preamble` | 9'190 | TXT-03 (eigene Chunk-Einheit) | ex, t |
| 6 | `body` + 11 Hierarchie-Elemente | 295'085 | STR-01/02/03, TXT-01/02 | n, en, ex |
| 7 | `num`/`heading`/`content`/`p`/`intro` | >1.6 M | TXT-01/02/03 | ex |
| 8 | `blockList`/`listIntroduction`/`item` | 351'050 | TXT-01/02 (Listen als Einheit) | ex |
| 9 | Inline-Markup `b`/`i`/`sup`/`sub`/`span`/`br`/`inline` | >500 k | TXT-* (Flattening, Invariante) | t |
| 10 | `ref` | 83'710 | REF-01/02 | en, ex |
| 11 | `authorialNote` | 77'349 | MOD-02 | en, ex |
| 12 | `mod`/`quotedStructure` | 12'481 | MOD-01 | en, ex |
| 13 | `components`/`component`/`doc`/`mainBody` | 5'195 | CMP-01/02 | en, n |
| 14 | `table`/`tr`/`td`/`th` | 1.26 M | SPC-01 | en, ex |
| 15 | `foreign` + SVG/MathML/SKOS/OOXML | 24 Dateien | SPC-02 | en |
| 16 | `signature` | 1'289 | TXT-03, CHK-02 (eigene Einheit) | ex |
| 17 | `@eId`-System | 597'278 | STR-02 (+ Invariante Normalisierung) | n |
| 18 | 5+1 Dokumentmuster | 15'807 | DOC-03 (Strategie-Weiche) | ex |
| 19 | Hollowing/Chunking (Querschnitt) | — | CHK-01/02 | t |

**Element-Audit.** Alle 147 vorkommenden Tags sind genau einer Gruppe zugeordnet oder stehen in der Ausschlussliste. Bei Format-Updates (neue Fedlex-Konverter-Releases) ist diese Matrix der Diff-Anker.

## Explizit ausgeschlossene Elemente

Kein Primitiv — mit empirischer Begründung. Ausschluss ist eine Entscheidung, kein Vergessen.

| Element/Attribut | Begründung | Beleg |
|------------------|------------|-------|
| 11 Dokumenttypen (`bill`, `judgment`, `debate`, `amendment`, …) | nie als Top-Level genutzt — 100 % `<act>` | X17.2 |
| 17 Hierarchie-Elemente (`alinea`, `clause`, `point`, `subsection`, `list`, …) | 0 Vorkommen im Corpus | X17.3 |
| 9 Meta-Sektionen (`lifecycle`, `analysis`, `temporalData`, `workflow`, …) | nie genutzt — Lebenszyklus liegt in JOLux | X17.4 |
| `conclusions`/`coverPage`/`attachments` | 0 Vorkommen — Anhänge laufen über `<components>` | X17.7 |
| `FRBRitem` | nie implementiert — das Item ist die Datei selbst | X19.1 |
| `@placement` auf `authorialNote` | zu 100 % absent (XSD-Violation V2) | X17.1 |
| `@contains` auf `act` | nie gesetzt (Default `originalVersion`) | X17.2 |
| `<inline name="man-*">` | reine Word-Konvertierungs-Präsentation — wird geflattet, nie exponiert | X18.5 |
| `<placeholder>`/`<block>`/`<container>` | Präsentations-Artefakte (placeholder→num 96.5 %, block→container 99.98 %, container→preface 98.2 %) | X16.5, X16.6 |
| ungültige `@status`-Werte (`annex`, `signature`) | 98 Vorkommen, tolerant ignorieren | X17.1 V3 |

## Kompositions-Karte

Typische Ketten, wie Konsumenten Primitive verbinden — inklusive der JOLux-Brücken:

```
Beschaffung         JLX-TMP-02 (Stichtag) → JLX-RES-04 (URL) → DOC-01 → DOC-02/03
                                          │
Struktur            DOC-03 → STR-01 (Outline) → STR-02 (eId) → STR-03 (Pfad)
                                          │
Normtext            STR-02 → TXT-01/02 (Artikel/Element) → syllogismus-fedlex
                    DOC-01 → TXT-03 (ganzes Dokument als Markdown)
                                          │
Änderungshistorie   TXT-01 → MOD-02 (Fussnoten) ⊕ JLX-IMP-02 (Graph) → Merge
                    DOC-03 (AMENDMENT) → MOD-01 (neue Wortlaute) ⊕ JLX-IMP-03
                                          │
Zitationsnetz       REF-01 ⊕ JLX-CIT-01 (Merge!) → REF-02 (href-lose) → JLX-RES-01
                                          │
Anhänge             CMP-01 ⊕ JLX-SUB-02 (Abgleich) → CMP-02 → rekursiv STR/TXT
                                          │
RAG-Ingest          DOC-03 → CHK-01 (Hollowing) → CHK-02 (Chunks) → semantic-fedlex
```

Drei Kompositions-Invarianten für Skill-Autoren:

1. **Muster zuerst.** Vor jedem Text-/Struktur-Zugriff DOC-03 fragen — 91 % der OC/FGA-Dateien haben keine Artikel, 34 % keinen Body.
2. **Hollowing vor Volltext.** Jede Operation über mehr als ein Element (Suche, Markdown, Chunks) braucht CHK-01 — sonst 87 % Redundanz.
3. **Historie und Zitationen sind immer Merges.** AKN-Notes ⊕ JOLux-Impacts, AKN-Refs ⊕ JOLux-Citations. Keine Seite allein ist vollständig.

---

## Statistik

| Kennzahl | Wert |
|---|---|
| Primitive total | **20** |
| davon implementiert (`crates/fedlex-akn`) | **20/20** |
| Offline-Tests (Mock-Fixture) | 31/31 grün (dazu 4 Brücken-Tests in `fedlex-bridge`) |
| E2E-Konformanz (Live-Fedlex, EnG + Stromgesetz-OC) | 12/12 grün (dazu 3 Brücken-E2E) |
| Tags abgedeckt | 147/147 (zugeordnet oder begründet ausgeschlossen) |
| Dokumentmuster | 6/6 (5 + OTHER) über DOC-03 |
| Meta-Sektionen | 2/2 genutzte (9 nie genutzte ausgeschlossen) |

---

## Konformanz-Suite

Wie beim JOLux-Lexikon ist jeder Eintrag **ausführbar spezifiziert**. Test-Ort ist
`crates/fedlex-akn/tests/lexicon_conformance.rs` (E2E, `#[ignore]`), dazu 31 Offline-Tests
gegen eine EnG-artige Mock-Fixture in den Modulen selbst.

```sh
cargo test -p fedlex-akn                                            # offline
cargo test -p fedlex-akn --test lexicon_conformance -- --ignored --test-threads 2   # live
cargo test -p fedlex-bridge --test bridge_conformance -- --ignored --test-threads 2 # Brücke live (jolux→akn-Komposition)
```

**Referenzdokumente** sind das EnG (SR 730.0, konsolidierte Stichtagsfassung via
SPARQL-FRBR-Kette aufgelöst, analog JLX-TMP-02 + JLX-RES-04) und für MOD-01 das
Stromgesetz `eli/oc/2024/679` (98 `<mod>`-Blöcke, XML live verifiziert). Eingelockte
Erwartungen aus der Analyse (Assertions tolerant als Bereiche, da die Konsolidierung wächst):

| Erwartung | Wert | Beleg |
|---|---|---|
| eIds total | 779 | X20.1 |
| Artikel | 105 | X20.1 |
| Blatt-Elemente | 655 (84.1 %) | X20.1 |
| Rohtext | ~117'647 Zeichen | X20.1 |
| Gehollowter Text | ~15'156 Zeichen | X20.1 |
| Muster | FLAT_ARTICLES oder STRUCTURED | X4.2 |
| FRBR-Kette | Work/Expression/Manifestation vollständig | X19.1 |

Drei Prinzipien (identisch zur JOLux-Suite):

1. **Capability-Beweis.** Jedes Primitiv liefert auf dem Referenzdokument Daten.
2. **Erwartungs-Beweis.** Falltraps werden als Assertions eingelockt (mod:quotedStructure 1:1, mainBody nie auf act-Ebene, placement immer absent, eId-Duplikat-Handling).
3. **Drift-Wache.** Ändert Fedlex den XML-Konverter (neue Tags, neue Muster), schlägt der Audit fehl und fordert einen Lexikon-Update.

**Lauf-Historie**

| Datum | Ergebnis | Befunde |
|---|---|---|
| 2026-06-10 (Brücke) | 3/3 Brücken-E2E + 4/4 offline grün | AKN-DOC-01 produktiv geschlossen. Neue Crate `fedlex-bridge` mit `HttpSparqlClient` (erste Produktions-Implementierung des `SparqlClient`-Traits), `XmlSource`-Trait + `HttpXmlSource` und `AknFetcher` (JLX-TMP-02 → Download → Parse, moka-Cache mit Manifestations-URL als Schlüssel, da immutable). E2E beweist die Komposition jolux→akn als Ganzes (DE + FR, NotFound vor Inkrafttreten). Keine neuen Live-Befunde. |
| 2026-06-10 (Review) | 12/12 E2E + 34/34 offline grün | 2 Bugs im Review gefunden und gefixt. (1) **Chunking-Datenverlust** — übergrosser Artikel ohne direkte `paragraph`-Kinder produzierte null Chunks; jetzt Fallback auf Ein-Artikel-Chunk. (2) **Instabile Chunk-IDs** — datierte Konsolidierungs-URIs flossen in `chunk_id`/`metadata.eli`/`Provenance`; jetzt `work_eli_path()`-Normalisierung auf datumslose Work-Ebene (Datum gehört in `ValidAsOf`/`date`). E2E-Assertions entsprechend verschärft (exakte Work-ELI statt Präfix). |
| 2026-06-10 | 12/12 E2E + 31/31 offline grün | 3 Live-Befunde, alle eingelockt. (1) Konsolidierungs-XML trägt **absolute, datierte** FRBR-Work-URIs (`https://…/eli/cc/2017/762/20260401`), nicht die relativen des Analyse-Snapshots — `eli_path()` normalisiert beide Formen. (2) `FRBRname` ist **mehrsprachig** (rm/it/de/fr als Geschwister, erstes Kind ist rm!) — Titel wird über die Expression-Sprache gewählt. (3) Im aktuellen EnG tragen **alle** refs ein href — die 15 % href-losen (X11.2) sind eine Corpus-Quote, keine Dokument-Garantie. Zudem: `eli/oc/2020/752` (Lexikon-Beispiel) hat KEIN XML — Manifestationen erst ab ~2021, MOD-Referenz ist deshalb `eli/oc/2024/679`. |

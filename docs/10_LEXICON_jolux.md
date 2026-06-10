# Capability-Lexikon JOLux — der vollständige Funktionsraum

> **Was dieses Dokument ist.** Die systematische, ontologie-getriebene Enumeration **aller** Operationen, die das JOLux-Datenmodell von Fedlex hergibt. Abgeleitet aus der offiziellen Ontologie (19 Klassen, 65 Prädikate, 46 SKOS-Vokabulare) und gefiltert durch die empirische Realität (Füllraten, verifiziert auf 69'350 CC-Einträgen). Es ist **kein** Tool-Katalog eines bestimmten Servers, sondern das Vokabular, aus dem Konsumenten komponieren.
>
> **Wer es konsumiert.**
> - `mcp-fedlex` implementiert Primitive als MCP-Tools (Projektion, nicht Quelle).
> - `skills-fedlex` komponiert Primitive zu juristischen Playbooks.
> - `syllogismus-fedlex` referenziert Primitive in auditierbaren Schluss-Schritten.
> - Orchestratoren (z.B. OpenClaw) planen über Lexikon-IDs statt über Tool-Namen.
>
> **Quellen (Ground Truth).**
> - Ontologie-Referenz: `fedlex-RAG-evaluation/macro-graphRAG-fedlex/fedlex-jolux/doc/reference.md`
> - Empirisches Rulebook J0–J20: `fedlex-RAG-evaluation/data_understanding/jolux/rulebook_jolux.md`
> - Implementierungs-Spec: `../../analyse-fedlex/10_DATA_RULES_jolux.md`
>
> **Pendant.** Der AKN-Funktionsraum (Volltext/Struktur) steht in `11_LEXICON_akn.md`. JOLux liefert Metadaten, niemals Gesetzestext (J0.1).

---

## Inhalt

- [Methodik und Vollständigkeitsbeweis](#methodik-und-vollständigkeitsbeweis)
- [Eintragsformat](#eintragsformat)
- [Domäne 1 — Identität & Auflösung (RES)](#domäne-1--identität--auflösung-res)
- [Domäne 2 — Temporalität & Versionen (TMP)](#domäne-2--temporalität--versionen-tmp)
- [Domäne 3 — Struktur-Referenzen (SUB)](#domäne-3--struktur-referenzen-sub)
- [Domäne 4 — Änderungsgraph (IMP)](#domäne-4--änderungsgraph-imp)
- [Domäne 5 — Zitationsgraph (CIT)](#domäne-5--zitationsgraph-cit)
- [Domäne 6 — Thematische Navigation (TAX)](#domäne-6--thematische-navigation-tax)
- [Domäne 7 — Publikations-Lebenszyklus (PUB)](#domäne-7--publikations-lebenszyklus-pub)
- [Domäne 8 — Entstehungsgeschichte (GEN)](#domäne-8--entstehungsgeschichte-gen)
- [Domäne 9 — Völkerrecht (TRT)](#domäne-9--völkerrecht-trt)
- [Domäne 10 — Vokabulare & Schema (VOC)](#domäne-10--vokabulare--schema-voc)
- [Vollständigkeits-Matrix](#vollständigkeits-matrix)
- [Explizit ausgeschlossene Prädikate](#explizit-ausgeschlossene-prädikate)
- [Kompositions-Karte](#kompositions-karte)
- [Statistik](#statistik)
- [Konformanz-Suite (der Test-Ort)](#konformanz-suite-der-test-ort)

---

## Methodik und Vollständigkeitsbeweis

Vollständigkeit wird hier **konstruktiv** behauptet und ist nachprüfbar:

1. **Soll:** Jede der 19 JOLux-Klassen (+ `Memorial`) und jedes der 65 Prädikate aus dem Rulebook wird genau einem Lexikon-Eintrag zugeordnet — oder explizit ausgeschlossen (mit empirischer Begründung, siehe [Ausschlussliste](#explizit-ausgeschlossene-prädikate)).
2. **Ist:** Jeder Eintrag trägt die empirische Füllrate/Volumetrie aus dem Rulebook. Primitive über leere Prädikate existieren nicht (sie würden konstant nichts liefern).
3. **Zugriffsmuster:** Pro Entität werden vier Muster geprüft. *lookup* (eine Instanz lesen), *enumerate* (Instanzen auflisten/filtern), *traverse-out* (ausgehende Kanten folgen), *traverse-in* (eingehende Kanten folgen). Nicht jedes Muster ergibt für jede Klasse ein eigenes Primitiv — wo Muster zusammenfallen, ist das im Eintrag vermerkt.

Die [Vollständigkeits-Matrix](#vollständigkeits-matrix) am Ende ist der Audit-Trail.

## Eintragsformat

```
### JLX-<DOM>-<NN> · <name>
Frage        die juristische/agentische Frage, die das Primitiv beantwortet
Signatur     (input) → output   — konzeptionell, nicht wire-format
JOLux        beteiligte Klassen & Prädikate
Empirie      Füllraten, Volumetrie, Verifikationsreferenz (J-Regel)
Falltraps    bekannte Fallen (Richtung, Phantome, Normalisierung)
Komposition  typische Vorgänger (←) und Nachfolger (→)
Status       PoC-validiert | abgeleitet (neu) | …
```

Querschnitt-Invarianten (gelten für **jedes** Primitiv, werden nicht wiederholt):

- **Provenance-Pflicht.** Jede Antwort trägt `{ eli, valid_as_of, transaction_time }` (ADR-004).
- **Stichtag explizit.** `as_of` optional, Default heute. Nie implizit "neueste Fassung".
- **OPTIONAL-Pflicht.** Kein Feld ist garantiert befüllt (J17.2). Fehlende Felder sind Daten, keine Fehler.
- **eId-Normalisierung** vor jedem JOLux↔AKN-Abgleich. `_([a-z])($|/)` → `$1$2` (J18.2).
- **Graceful Failure.** `{ error, hint }`, nie Crash.

---

## Domäne 1 — Identität & Auflösung (RES)

Das Identitätssystem. ELI-URIs, SR-Nummern, FRBR-Achsen (Work → Expression → Manifestation).

### JLX-RES-01 · resolve_sr_number
- **Frage:** "Welches Gesetz ist SR 730.0?"
- **Signatur:** `(sr_number, as_of?) → [{ eli, title, type, in_force }]`
- **JOLux:** `ConsolidationAbstract.historicalLegalId`
- **Empirie:** SR-Nummern auf allen CC-CAs. Juristen referenzieren primär über SR, nicht ELI.
- **Falltraps:** **SR-Nummern werden wiederverwendet** — 730.0 zeigt auf das alte EnG (`eli/cc/1999/27`, aufgehoben) UND das neue (`eli/cc/2017/762`). Rückgabe ist eine **Liste**, Disambiguierung über Geltung (TMP-03). ⚡ *Live-verifiziert 2026-06-10.* Staatsverträge haben SR 0.xxx (J12.4). FGA/OC-Einträge haben **keine** SR-Nummer (J9.3).
- **Komposition:** → JLX-TMP-03 (Disambiguierung), dann jedes andere Primitiv
- **Status:** implementiert + konformanz-getestet (`resolve_sr_number`, `jlx_res_01`)

### JLX-RES-02 · search_law
- **Frage:** "Finde das Energiegesetz" / "Gesetze über X"
- **Signatur:** `(query, lang?, type?, limit?) → [{ eli, sr, title, type }]`
- **JOLux:** `Expression.title`, `Expression.titleShort`, `historicalLegalId`, `typeDocument`
- **Empirie:** 69'350 CAs. 23 Dokumenttypen (J15.1). PoC-Erfolgsquote 97 %.
- **Falltraps:** **Titel liegt auf der CA-direkten Expression** (`<CA> isRealizedBy ?expr`). Die Expressions der Consolidations tragen nur technische Labels ("Consolidation: 730.0 - 2018-01-01") — wer dort sucht, findet nichts. ⚡ *Live-verifiziert 2026-06-10 (Bug in früher Implementierung gefunden und behoben).* `titleShort` oft leer (J3.4).
- **Komposition:** → JLX-RES-03, JLX-TMP-01
- **Status:** implementiert + konformanz-getestet (`search_law`, `jlx_res_02`)

### JLX-RES-03 · get_law_metadata
- **Frage:** "Was ist dieses Gesetz?" (Steckbrief)
- **Signatur:** `(eli, as_of?) → { sr, title, type, dates { document, entry_in_force, no_longer_in_force }, in_force_status, taxonomy[], basic_act, languages[] }`
- **JOLux:** Die 14 direkten CA-Prädikate (J1.2). `historicalLegalId`, `dateDocument`, `dateEntryInForce`, `dateNoLongerInForce`, `inForceStatus`, `typeDocument`, `historicalTypeDocument`, `classifiedByTaxonomyEntry`, `basicAct`, …
- **Empirie:** 15.1 % der CAs **ohne** `inForceStatus` (J3.3). Nur Status 0/1/3 genutzt, 2/4/5 sind tote Vokabular-Einträge.
- **Falltraps:** `legalResourceGenre`/`responsibilityOf` auf CA-Ebene **leer** (0/69'350) — für Autor/Genre → JLX-PUB-01 (OC-Act). `dateApplicability` auf CA-Ebene Phantom (J3.1).
- **Komposition:** ← JLX-RES-01/02 | → JLX-TMP-*, JLX-IMP-*, AKN-Layer
- **Status:** implementiert + konformanz-getestet (`get_law_metadata`, `jlx_res_03`)

### JLX-RES-04 · resolve_manifestation
- **Frage:** "Gib mir die Download-URL (XML/PDF/HTML) der Fassung X in Sprache Y"
- **Signatur:** `(eli, as_of?, lang?, format?) → { url, format, consolidation_date }`
- **JOLux:** FRBR-Kette `?cons isMemberOf <CA>` → `dateApplicability` → `isRealizedBy` → `language` → `isEmbodiedBy` → `isExemplifiedBy` (J2.1, J2.2)
- **Empirie:** 3 Sprachen × 4 Formate = 12 Manifestationen pro Consolidation (J19.5). XML ab ~2021, davor nur PDF-A (J14.2).
- **Falltraps:** **FRBR-Richtung.** `?cons jolux:isMemberOf <CA>` ist eingehend — `<CA> isRealizedBy ?cons` liefert 0 Ergebnisse (J2.1). FGA hat verkürzte Kette ohne Consolidation (J9.3).
- **Komposition:** ← JLX-RES-03, JLX-TMP-01 | → AKN-Layer (einzige Brücke zum Text, J18.1 Typ 1)
- **Status:** implementiert + konformanz-getestet (`resolve_manifestation`, `jlx_res_04`)

### JLX-RES-05 · list_expressions
- **Frage:** "In welchen Sprachen existiert dieses Gesetz?"
- **Signatur:** `(eli, as_of?) → [{ lang, title, formats[] }]`
- **JOLux:** `isRealizedBy`, `language`, `isEmbodiedBy`
- **Empirie:** DE/FR/IT flächendeckend (je ~5'854 mit XML), EN nur 290, RM 85 (J13.1). eIds sind 100 % sprachinvariant (J13.2).
- **Falltraps:** FR-Texte ~12 % länger als DE (Token-Budget, J13.3).
- **Komposition:** ← JLX-RES-03 | → JLX-RES-04
- **Status:** implementiert + konformanz-getestet (`list_expressions`, `jlx_res_05`)

---

## Domäne 2 — Temporalität & Versionen (TMP)

Bi-Temporalität ist das Kernversprechen. JOLux modelliert sie über Consolidations.

### JLX-TMP-01 · list_versions
- **Frage:** "Welche Fassungen dieses Gesetzes existieren?"
- **Signatur:** `(eli) → [{ consolidation_eli, date_applicability, has_xml }]`
- **JOLux:** `?cons isMemberOf <CA>`, `Consolidation.dateApplicability`
- **Empirie:** Versionsanzahl stark typabhängig. Bundesgesetz Ø 12.3 (max 118), bilateraler Staatsvertrag Ø 1.5 (J14.1b). EnG: 13 Fassungen, 11 mit XML.
- **Falltraps:** Fassungen vor ~2021 nur PDF-A (J14.2). 6'532 CAs ganz ohne Consolidations (J3.3).
- **Komposition:** ← JLX-RES-03 | → JLX-TMP-02, JLX-RES-04
- **Status:** implementiert + konformanz-getestet (`list_versions`, `jlx_tmp_01`)

### JLX-TMP-02 · resolve_version_at
- **Frage:** "Welche Fassung galt am Stichtag X?"
- **Signatur:** `(eli, as_of) → { consolidation_eli, date_applicability }`
- **JOLux:** `FILTER(?date <= as_of) ORDER BY DESC(?date) LIMIT 1` (J14.3)
- **Empirie:** 55'981 Consolidations mit `dateApplicability` (J20.2).
- **Falltraps:** `dateEntryInForce` (CA) und `dateApplicability` (Consolidation) können bis **927 Tage** divergieren (J20.2) — für juristische Präzision beide prüfen.
- **Komposition:** ← jedes `as_of`-tragende Primitiv (interner Baustein des Temporal Resolvers)
- **Status:** implementiert + konformanz-getestet (`resolve_consolidation_at`, `jlx_tmp_02`)

### JLX-TMP-03 · check_in_force
- **Frage:** "Gilt dieses Gesetz (heute / am Stichtag)?"
- **Signatur:** `(eli, as_of?) → { in_force, status_label, since, until? }`
- **JOLux:** `inForceStatus`, `dateEntryInForce`, `dateNoLongerInForce`, `dateEndApplicability`
- **Empirie:** 47.6 % in Kraft, 52 % ausser Kraft, 15.1 % ohne Status (J3.3). `dateNoLongerInForce` deckt 96 % der Abgelaufenen, `dateEndApplicability` nur Sonderfälle (4 %).
- **Falltraps:** Status-Feld allein genügt nicht (10'479 CAs ohne Status, aber mit `dateEntryInForce`). Doppel-FILTER nach J3.2 verwenden. In 4 % liegt `dateNoLongerInForce` **vor** `dateEndApplicability`.
- **Komposition:** ← JLX-RES-03 | Pflicht-Baustein jeder Geltungs-Aussage in syllogismus-fedlex
- **Status:** implementiert + konformanz-getestet (`check_in_force`, `jlx_tmp_03`)

---

## Domäne 3 — Struktur-Referenzen (SUB)

JOLux kennt Textstruktur nur als Lückenkatalog. Diese Domäne ist bewusst klein — Vollstruktur liefert AKN.

### JLX-SUB-01 · get_subdivisions
- **Frage:** "Welche Artikel/Kapitel dieses Gesetzes kennt der Graph?"
- **Signatur:** `(eli, type?) → [{ uri, eid, type, part_of }]`
- **JOLux:** `LegalResourceSubdivision`, `legalResourceSubdivisionIsPartOf` (transitiv mit `+`, J17.3), `legalResourceSubdivisionType`
- **Empirie:** **Abdeckung 0.4–8.5 %** der XML-eIds — nur Elemente mit ≥1 Impact existieren (J4.1). Absätze/Items/Sections: 0 %. 18 von 24 Subdivision-Typen genutzt (Erlass 91.5 %, Artikel 5.4 %, Anhang 0.5 %).
- **Falltraps:** Je älter das Gesetz, desto schlechter die Abdeckung (BV 0.4 %). Zählungen variieren je Query-Methode (J4.5b). **Niemals** als "Inhaltsverzeichnis" verkaufen — dafür AKN `get_document_structure`.
- **Komposition:** ← JLX-RES-03 | → JLX-IMP-02 (Subdivision-URIs sind Impact-Ziele)
- **Status:** implementiert + konformanz-getestet (`get_subdivisions`, `jlx_sub_01`)

### JLX-SUB-02 · list_annexes
- **Frage:** "Hat dieses Gesetz Anhänge?"
- **Signatur:** `(eli) → [{ uri, eid }]`
- **JOLux:** Subdivision-Typ `annex`
- **Empirie:** 1'611 Annex-Subdivisions total, 1'266 bei CC-CAs (500 CAs haben Annexe) (J18.2b).
- **Falltraps:** Annexe erscheinen im AKN-XML als `<component>`, nicht `<attachment>` (J18.2b). ⚡ Der Referenz-Erlass EnG hat **keine** Annex-Subdivisions — die Suite prüft die Existenz deshalb systemweit (*Live-Befund 2026-06-10*).
- **Komposition:** ← JLX-RES-03 | → AKN-Layer (Component-Auflösung)
- **Status:** implementiert + konformanz-getestet (`list_annexes`, `jlx_sub_02`)

---

## Domäne 4 — Änderungsgraph (IMP)

Die formale Änderungshistorie. `OC-Erlass → Impact → CC-Subdivision` (J6.1).

### JLX-IMP-01 · get_impacts
- **Frage:** "Wie wurde dieses Gesetz über die Zeit geändert?"
- **Signatur:** `(eli, since?, until?) → [{ impact, source_act, target { subdivision | comment }, type, date_entry_in_force, source_system }]`
- **JOLux:** `LegalResourceImpact`, `impactFromLegalResource`, `impactToLegalResource`, `legalResourceImpactHasType` (28 Typen. Änderung 56.5 %, Inkrafttreten 16.4 %, Aufhebung 8.7 %), `legalResourceImpactHasDateEntryInForce`, `impactConsolidatedBy`, `informationSource`
- **Empirie:** 306'526 Impacts. Drei Quellsysteme koexistieren (geschaeftsstaende 63.9 %, mutation 26.9 %, legiconso 9.2 %) (J6.2).
- **Falltraps:** **Systembruch 2023.** Seither dominiert wieder Freitext (`impactToLegalResourceComment`, "Art. 5, 7, 12") statt strukturierter Subdivisions — Comment-Parsing ist Pflicht, kein Nice-to-have (J6.4). 38'701 Impacts NUR als Comment.
- **Komposition:** ← JLX-RES-03 | → JLX-IMP-02, JLX-PUB-01 (source_act ist OC)
- **Status:** implementiert + konformanz-getestet (`get_impacts`, `jlx_imp_01`)

### JLX-IMP-02 · get_article_history
- **Frage:** "Wann wurde Art. X von wem wie geändert?"
- **Signatur:** `(eli, eid) → [{ type, date, source_act, comment? }]`
- **JOLux:** Impacts gefiltert auf Subdivision-URI (normalisierte eId)
- **Empirie:** Einzige Quelle für **historisch aufgehobene Artikel** — das aktuelle AKN-XML enthält sie nicht mehr (OR: 36 aufgehobene Artikel) (J6.5).
- **Falltraps:** eId-Normalisierung zwingend (`art_14_a` → `art_14a`, J18.2). Nach 2023 ggf. nur via Comment-Parsing auffindbar.
- **Komposition:** ← JLX-SUB-01 oder AKN-eId | → AKN `get_article_text` (Vorher/Nachher-Vergleich)
- **Status:** implementiert + konformanz-getestet (`get_article_history`, `jlx_imp_02`)

### JLX-IMP-03 · get_outgoing_impacts
- **Frage:** "Welche Gesetze ändert dieser Erlass?" (Richtung umgekehrt zu IMP-01)
- **Signatur:** `(oc_eli) → [{ target_law, target_subdivision?, type, date }]`
- **JOLux:** `impactFromLegalResource` als Einstieg (traverse-out vom OC-Act)
- **Empirie:** Ein Änderungserlass betrifft typischerweise mehrere CC-Gesetze (J8.4). Mantelerlasse (855 im FGA) bündeln viele Änderungen.
- **Falltraps:** Nur OC/FGA-Erlasse sind Impact-Quellen, nie CC-Einträge.
- **Komposition:** ← JLX-PUB-01 | → JLX-RES-03 (Ziel-Gesetze auflösen)
- **Status:** implementiert + konformanz-getestet (`get_outgoing_impacts`, `jlx_imp_03`)

---

## Domäne 5 — Zitationsgraph (CIT)

### JLX-CIT-01 · get_citations
- **Frage:** "Welche Gesetze zitiert X / wer zitiert X?"
- **Signatur:** `(eli, direction: outgoing|incoming|both) → [{ from, to, description? }]`
- **JOLux:** `Citation`, `citationFromLegalResource`, `citationToLegalResource`, `descriptionFrom`
- **Empirie:** Nur Gesamttext-Granularität (`/text`), nie Artikel-Ebene (J7.1). Overlap mit AKN-Inline-`<ref>` nur **0–48 %** (J7.3) — beide Quellen sind nötig, keine ersetzt die andere.
- **Falltraps:** Rulebook J7.2 ("descriptionFrom systematisch leer") ist **überholt** — Fedlex befüllt das Feld inzwischen mit Artikel-Beschreibungen. ⚡ *Live-verifiziert 2026-06-10.* Weiterhin als `Option` behandeln. Duplikate pro Fassung → nach Quellgesetz deduplizieren (J7.4). ⚡ Die Fedlex-WAF blockt `SELECT DISTINCT` in Kombination mit `citationFromLegalResource` und einem URL-Literal (HTTP 400, SQL-Injection-Heuristik) — `get_citations` fragt ohne DISTINCT ab und dedupliziert clientseitig (*Live-Befund 2026-06-10*).
- **Komposition:** ← JLX-RES-03 | ⊕ AKN `get_all_references` (Merge ist die eigentliche Capability)
- **Status:** implementiert + konformanz-getestet (`get_citations`, `jlx_cit_01`)

---

## Domäne 6 — Thematische Navigation (TAX)

### JLX-TAX-01 · get_taxonomy
- **Frage:** "In welchem Rechtsgebiet steht dieses Gesetz?"
- **Signatur:** `(eli) → [{ taxonomy_uri, label, broader[] }]`
- **JOLux:** `classifiedByTaxonomyEntry`, SKOS `legal-taxonomy` (12'227 Einträge), `skos:broader`-Hierarchie
- **Empirie:** 85.4 % der CAs klassifiziert, 9'027 verschiedene Einträge (J20.3). 10'132 CAs ohne Eintrag.
- **Falltraps:** Labels sind opake URIs ohne SKOS-Lookup (J5.3).
- **Komposition:** ← JLX-RES-03 | → JLX-TAX-02
- **Status:** implementiert + konformanz-getestet (`get_taxonomy`, `jlx_tax_01`)

### JLX-TAX-02 · find_related_by_topic
- **Frage:** "Welche Gesetze gehören zum selben Rechtsgebiet?"
- **Signatur:** `(taxonomy_uri | eli, depth?) → [{ eli, title, sr }]`
- **JOLux:** `classifiedByTaxonomyEntry` invers + `skos:broader`/`narrower`
- **Empirie:** **Die** deterministische Brücke für Cross-Law-Navigation (J20.3) — komplementär zu semantic-fedlex (Embedding-basiert, probabilistisch).
- **Falltraps:** Taxonomie deckt 85.4 % — Rest nur über Vektor-Suche erreichbar.
- **Komposition:** ← JLX-TAX-01 | alternative zu `semantic_search` bei Makro-Scope
- **Status:** implementiert + konformanz-getestet (`find_related_by_topic`, `jlx_tax_02`)

---

## Domäne 7 — Publikations-Lebenszyklus (PUB)

OC (rechtsverbindlich) und FGA (Kontext). Strukturell anders als CC (J9.3).

### JLX-PUB-01 · get_oc_act
- **Frage:** "Was ist der rechtsverbindliche Grundakt? Wer ist Autor?"
- **Signatur:** `(eli) → { oc_eli, publication_date, sequence, completeness, genre, responsible_office, memorial }`
- **JOLux:** `basicAct` (CA → OC), `Act`, `publicationDate`, `sequenceInTheYearOfPublication`, `legalResourcePublicationCompleteness`, `legalResourceGenre`, `responsibilityOf`, `isPartOf` (→ Memorial)
- **Empirie:** **CC ist NICHT rechtsverbindlich** — die OC ist es (J19.2). Genre 99.6 % und Autor 47.4 % nur hier befüllt (J8.3). Vollständigkeit 97.4 %.
- **Falltraps:** CC-URI ist deterministisch aus BasicAct ableitbar (`oc→cc`, `fga→cc+_fga`-Suffix, J19.1) — die Gegenrichtung braucht keinen Lookup.
- **Komposition:** ← JLX-RES-03 (`basicAct`) | → JLX-IMP-03, JLX-PUB-02
- **Status:** implementiert + konformanz-getestet (`get_oc_act`, `jlx_pub_01`)

### JLX-PUB-02 · get_memorial
- **Frage:** "In welchem AS/BBl-Wochenbulletin wurde das publiziert?"
- **Signatur:** `(oc_eli | memorial_uri) → { memorial_uri, year, number, acts[] }`
- **JOLux:** `Memorial` (238'107 Instanzen, 1849–2026), `isPartOf`, URI-Muster `eli/collection/{oc|fga}/YYYY/NN` (J19.3)
- **Empirie:** Vollständige Bulletin-Historie seit 1849.
- **Falltraps:** `Memorial` fehlt in der 19-Klassen-Liste des Rulebooks (separat in J19.3 verifiziert) — hier bewusst als 20. Entität geführt.
- **Komposition:** ← JLX-PUB-01 | enumerate-Richtung. alle Erlasse eines Bulletins
- **Status:** implementiert + konformanz-getestet (`get_memorial`, `jlx_pub_02`)

### JLX-PUB-03 · get_fga_documents
- **Frage:** "Welche Botschaften/Berichte gehören zu diesem Gesetz?"
- **Signatur:** `(eli | query) → [{ fga_eli, genre, date, title }]`
- **JOLux:** FGA-`Act`s, `legalResourceGenre` (95.2 % Kontextdokumente, J9.1), verkürzte FRBR-Kette ohne Consolidation (J9.3)
- **Empirie:** Beantwortet "Warum wurde dieses Gesetz erlassen?" (J9.2). Keine SR-Nummern, keine Impacts/Citations/Subdivisions auf FGA.
- **Falltraps:** FGA hat nur 7 der 17 CC-Prädikate. Nie `inForceStatus` auf FGA erwarten.
- **Komposition:** ← JLX-GEN-01 (Draft → FGA-Dokumente) | → JLX-RES-04 (XML der Botschaft)
- **Status:** implementiert + konformanz-getestet (`get_fga_documents`, `jlx_pub_03`)

---

## Domäne 8 — Entstehungsgeschichte (GEN)

Drafts und Vernehmlassungen. Der politische Prozess vor der Publikation (J18.3).

### JLX-GEN-01 · get_drafts
- **Frage:** "Wie kam dieses Gesetz zustande?" (parlamentarischer Prozess)
- **Signatur:** `(eli) → [{ draft_uri, draft_id, parliament_draft_id, process_type, tasks[], resulting_resource }]`
- **JOLux:** `Draft`, `draftId`, `parliamentDraftId`, `draftHasLegislativeTask`, `hasSubTask`, `processType`, `hasResultingLegalResource`
- **Empirie:** 85'996 Draft-Prozesse seit ~1848 (J11.1). `parliamentDraftId` (z.B. "13.074") ist der **Schlüssel zu Curia Vista** (J11.2) — Föderations-Hook für externe Parlamentsdaten.
- **Falltraps:** Traverse-in über `hasResultingLegalResource` — vom Gesetz rückwärts zum Draft.
- **Komposition:** ← JLX-RES-03 | → JLX-GEN-02, JLX-PUB-03, extern Curia Vista
- **Status:** implementiert + konformanz-getestet (`get_drafts`, `jlx_gen_01`)

### JLX-GEN-02 · get_consultations
- **Frage:** "Welche Vernehmlassung lief zu diesem Entwurf? Wer war federführend?"
- **Signatur:** `(eli | draft_uri) → [{ consultation_uri, status, start, end, institution, institution_l2, phases[], documents[] }]`
- **JOLux:** `Consultation` (2'514), `ConsultationTask` (6'279), `ConsultationPhase` (2'459), `ConsultationPreparation`, `consultationStatus`, `eventStartDate/EndDate`, `institutionInChargeOfTheEvent(/Level2)`
- **Empirie:** 91.6 % abgeschlossen. URI-Muster `eli/dl/proj/YYYY/ID/cons_N` 100 % konsistent (J20.4).
- **Falltraps:** **Consultation ↔ Phase läuft NIE direkt**, sondern über `ConsultationTask` als Zwischenknoten — direkte Query liefert 0 Treffer (J10.2). ⚡ Termine und Federführung liegen nicht auf der Consultation selbst, sondern auf deren `hasSubTask`-Tasks (*Live-Befund 2026-06-10*).
- **Komposition:** ← JLX-GEN-01 (`draftHasConsultation`) | → JLX-GEN-03
- **Status:** implementiert + konformanz-getestet (`get_consultations`, `jlx_gen_02`)

### JLX-GEN-03 · get_consultation_documents
- **Frage:** "Stellungnahmen und Ergebnisbericht der Vernehmlassung X?"
- **Signatur:** `(consultation_uri, doc_type?) → [{ uri, type_code, type_label, institution? }]`
- **JOLux:** `PositionStatementPublication` (873), `ResultOfAConsultationPublication` (1'789), `isOpinionOf`, `opinionIsAboutDraftDocument`, `opinionHasDraftRelatedDocument`, Typ-Codes 11/12/14/21/31 (J20.5)
- **Empirie:** Superkategorie-Logik der Codes. 10er = Phase, 20er = Stellungnahmen, 30er = Ergebnis.
- **Falltraps:** `opinionHasDraftRelatedDocument` ist 3.7× häufiger als `opinionIsAboutDraftDocument` — beide abfragen.
- **Komposition:** ← JLX-GEN-02 | → JLX-RES-04 (PDF der Stellungnahme)
- **Status:** implementiert + konformanz-getestet (`get_consultation_documents`, `jlx_gen_03`)

---

## Domäne 9 — Völkerrecht (TRT)

### JLX-TRT-01 · get_treaty_info
- **Frage:** "Mit wem wurde dieser Staatsvertrag geschlossen? Wann, wo, welcher Art?"
- **Signatur:** `(eli) → { process_uri, treaty_doc, signature { date, place }, bilateral, type, subject, parties { countries[], organisations[] }, status, approbation_act }`
- **JOLux:** `TreatyProcess` (19'830), `TreatyDocument`, `TaskForTreaty`, `treatyProcessHasTask`, `treatyProcessHasResultingTreatyDocument`, `treatySignatureDate/Place`, `bilateral`, `treatyType`, `treatySubject`, `treatyPartyCountry`, `treatyPartyOrganisation`, `treatyStatus`, `approbationAct`, `titleTreaty`
- **Empirie:** 96.7 % Grundverträge, 31.7 % bilateral (J12.1–12.2). Viele Verträge haben SR 0.xxx und volle CC-Behandlung (J12.4).
- **Falltraps:** `approbationAct` verknüpft zum Genehmigungs-Bundesbeschluss — eigene Erlass-Kette.
- **Komposition:** ← JLX-RES-01 (SR 0.xxx) | → JLX-RES-03 (Approbations-Erlass), JLX-TRT-02
- **Status:** implementiert + konformanz-getestet (`get_treaty_info`, `jlx_trt_01`)

### JLX-TRT-02 · find_treaties
- **Frage:** "Alle Verträge mit Land X / zum Thema Y?"
- **Signatur:** `(country? | subject? | type?, bilateral?) → [{ eli, title, signature_date, parties }]`
- **JOLux:** enumerate über `treatyPartyCountry` (Vokabular `country`, 429 Einträge), `treatySubject`, `treatyType`
- **Empirie:** Top-Sachgebiete. Entwicklung (4'005), Verkehr (2'415), Zoll (2'163), Steuern (1'892) (J12.3).
- **Falltraps:** 1.6 % ohne Bilateral-Flag.
- **Komposition:** → JLX-TRT-01
- **Status:** implementiert + konformanz-getestet (`find_treaties`, `jlx_trt_02`)

---

## Domäne 10 — Vokabulare & Schema (VOC)

JOLux-Werte sind opake URIs. Ohne SKOS-Auflösung ist alles andere bedeutungslos (J5.3).

### JLX-VOC-01 · resolve_vocabulary_term
- **Frage:** "Was bedeutet `resource-type/21`?"
- **Signatur:** `(term_uri, lang?) → { label, definition?, broader?, scheme }`
- **JOLux:** SKOS `prefLabel`/`broader` über 46 Kataloge, 49'184 Einträge
- **Empirie:** Alle 46 Kataloge live auflösbar, 40 mit DE-Labels. 6 nur EN/FR/IT (J5.4) — Fallback-Sprachkette nötig.
- **Falltraps:** `LANG()`-Filter ohne Fallback verliert die 6 fremdsprachigen Kataloge.
- **Komposition:** ← praktisch jedes Primitiv (Label-Auflösung) — Kandidat für transparente Middleware statt Agent-Tool
- **Status:** implementiert + konformanz-getestet (`resolve_vocabulary_label`, `jlx_voc_01`)

### JLX-VOC-02 · list_vocabulary
- **Frage:** "Welche Dokumenttypen / Impact-Typen / Status-Werte gibt es überhaupt?"
- **Signatur:** `(scheme_id, lang?) → [{ uri, code, label }]`
- **JOLux:** enumerate eines SKOS-Schemes (z.B. `resource-type` 23 genutzte Codes, `impact-type` 28 Typen, `subdivision-type` 18 genutzte)
- **Empirie:** Top-Kataloge. legal-taxonomy 12'227, jurivoc 10'799, legal-institution 703 (J5.2).
- **Falltraps:** Vokabular definiert ≠ empirisch genutzt (enforcement-status. 6 definiert, 3 genutzt). Das Lexikon liefert die Ist-Nutzung mit.
- **Komposition:** → Eingabe-Validierung aller filternden Primitive | ideal als **MCP Resource** (statisch, stabil) statt Tool
- **Status:** implementiert + konformanz-getestet (`list_vocabulary`, `jlx_voc_02`)

### JLX-VOC-03 · explore_node
- **Frage:** "Was hängt an diesem URI?" (Schema-Introspektion, Debug)
- **Signatur:** `(uri, direction?) → { outgoing: [{p, o}], incoming: [{s, p}] }`
- **JOLux:** generisches Triple-Browsing, klassenagnostisch
- **Empirie:** ~90 direkte Kanten pro Gesetz, 200+ Knoten im Teilgraph (J0.3).
- **Falltraps:** `rdf:type` kann 28× dupliziert erscheinen (Quad-Store-Artefakt, J16.2). Pagination zwingend.
- **Komposition:** Escape-Hatch, wenn kein spezifisches Primitiv passt. Sicherheitsnetz für Ontologie-Drift
- **Status:** implementiert + konformanz-getestet (`explore_node`, `jlx_voc_03`)

---

## Vollständigkeits-Matrix

Jede Klasse → Primitive. (l)ookup, (e)numerate, (ti) traverse-in, (to) traverse-out.

| # | Klasse | Primitive | Muster abgedeckt |
|---|--------|-----------|------------------|
| 1 | ConsolidationAbstract | RES-01/02/03, TMP-03, TAX-01 | l, e, to |
| 2 | Consolidation | TMP-01/02, RES-04 | l, e, ti (isMemberOf) |
| 3 | Expression | RES-05, RES-04 | l, e |
| 4 | Manifestation | RES-04 | l, to (isExemplifiedBy) |
| 5 | Work | RES-03 (impliziert; CA/Act sind die Work-Subtypen) | l |
| 6 | Act (OC) | PUB-01, IMP-03 | l, to |
| 7 | LegalResourceImpact | IMP-01/02/03 | l, e, ti, to |
| 8 | LegalResourceSubdivision | SUB-01/02, IMP-02 | l, e, ti, to |
| 9 | Citation | CIT-01 | e, ti, to |
| 10 | Consultation | GEN-02 | l, e |
| 11 | ConsultationTask | GEN-02 (Zwischenknoten) | to |
| 12 | ConsultationPhase | GEN-02 | l |
| 13 | ConsultationPreparation | GEN-03 | e |
| 14 | PositionStatementPublication | GEN-03 | e |
| 15 | ResultOfAConsultationPublication | GEN-03 | e |
| 16 | Draft | GEN-01 | l, e, ti, to |
| 17 | TaskForTreaty | TRT-01 | to |
| 18 | TreatyDocument | TRT-01 | l |
| 19 | TreatyProcess | TRT-01/02 | l, e, to |
| 20 | Memorial (J19.3) | PUB-02 | l, e, ti |
| — | 46 SKOS-Schemes | VOC-01/02 | l, e |
| — | beliebiger Knoten | VOC-03 | l, ti, to |

**Prädikat-Audit.** Alle 65 Prädikate des Rulebooks sind genau einem Primitiv zugeordnet oder stehen in der Ausschlussliste. Bei Ontologie-Updates (neue Fedlex-Releases) ist diese Matrix der Diff-Anker.

## Explizit ausgeschlossene Prädikate

Kein Primitiv — mit empirischer Begründung. Ausschluss ist eine Entscheidung, kein Vergessen.

| Prädikat | Begründung | Beleg |
|----------|------------|-------|
| `legalResourceLegalBasis` | Phantom. 0 Treffer auf CC+OC | J20.1, live bestätigt 2026-06 |
| `dateApplicability` (auf CA) | Phantom. 3/69'186 (auf Consolidation hingegen Kern → TMP-01/02) | J3.1, live bestätigt 2026-06 |
| `foreseenImpactToLegalResource` | 0.8 % aller Impacts, ohne Typ/Datum | J16.1 |
| `legalResourceGenre` / `responsibilityOf` **auf CA** | 0/69'350 — auf OC-Act befüllt → dort in PUB-01 | J1.2, J8.3, live bestätigt 2026-06 |
| `dct:identifier` | interne Dossier-Nummer | J16.1 |
| `titleShort` als Suchschlüssel | oft leer — nur als optionales Ausgabefeld (RES-02) | J3.4 |

**Revidierte Ausschlüsse** (Live-Befunde 2026-06-10, Rulebook überholt):

| Prädikat | Alt (Rulebook) | Neu (live) | Konsequenz |
|----------|----------------|------------|------------|
| `descriptionFrom` | systematisch leer (J7.2) | **befüllt** mit Artikel-Beschreibungen | in CIT-01 aufgenommen |
| `cogni:firstPublicationDate`, `cogni:skipIndex` | nicht im öffentlichen Endpoint (J16.2) | **vorhanden** auf CA-Knoten | weiterhin kein Primitiv (interne Felder), aber bei VOC-03 sichtbar |

## Kompositions-Karte

Typische Ketten, wie Konsumenten (skills-fedlex, Orchestratoren) Primitive verbinden:

```
Einstieg            RES-01 (SR) ─┐
                    RES-02 (Suche) ─┴→ RES-03 (Steckbrief)
                                          │
Geltung             RES-03 → TMP-03 (in Kraft?) → TMP-02 (Stichtagsfassung)
                                          │
Text (→ AKN)        TMP-02 → RES-04 (XML-URL) → AKN: get_document_structure / get_article_text
                                          │
Änderungshistorie   RES-03 → IMP-01 → IMP-02 (Artikel) → AKN-Versionen vergleichen
                                          │
Beziehungsnetz      RES-03 → CIT-01 ⊕ AKN-Refs (Merge!) / TAX-02 (Themen-Nachbarn)
                                          │
Entstehung          RES-03 → GEN-01 (Draft) → GEN-02 (Vernehmlassung) → GEN-03 (Dokumente)
                              └→ PUB-03 (Botschaft im BBl)
                                          │
Verbindlichkeit     RES-03 → PUB-01 (OC-Grundakt = rechtsverbindlich) → PUB-02 (Memorial)
                                          │
Völkerrecht         RES-01 (SR 0.xxx) → TRT-01 → RES-03 (Approbations-Erlass)
```

Drei Kompositions-Invarianten für Skill-Autoren:

1. **Geltungs-Aussagen** brauchen immer TMP-03 (nie nur Status-Feld lesen).
2. **Zitations-Vollständigkeit** braucht immer den Merge JOLux ⊕ AKN (Overlap nur 0–48 %).
3. **Struktur-Vollständigkeit** gibt es in JOLux nicht (max. 8.5 %) — Struktur-Fragen gehen an den AKN-Layer.

---

## Statistik

| Kennzahl | Wert |
|---|---|
| Primitive total | **27** |
| davon in Rust implementiert | **27/27** (alle Ende-zu-Ende getestet) |
| Klassen abgedeckt | 20/20 (19 + Memorial) |
| Prädikate zugeordnet | 65/65 (59 in Primitiven, 6 begründet ausgeschlossen, 2 Ausschlüsse live revidiert) |
| SKOS-Vokabulare | 46/46 über VOC-01/02 |
| Offline-Tests (Mock) | 44/44 pass |
| Konformanz-Tests | 29/29 pass (Live E2E, 2026-06-10) |

---

## Konformanz-Suite (der Test-Ort)

Dieses Lexikon ist **ausführbar spezifiziert**. Jeder Eintrag hat einen Live-Test in
[`crates/fedlex-jolux/tests/lexicon_conformance.rs`](../crates/fedlex-jolux/tests/lexicon_conformance.rs)
(ein Test pro JLX-ID plus Phantom-Audits für die Ausschlussliste).

```sh
cargo test -p fedlex-jolux --test lexicon_conformance -- --ignored --test-threads 2
```

Die Tests sind `#[ignore]`, damit `cargo test` offline bleibt. Endpoint via
`FEDLEX_SPARQL_ENDPOINT` überschreibbar (z.B. lokales Oxigraph).

Drei Prinzipien:

1. **Capability-Beweis.** Das SPARQL-Muster jedes Primitivs liefert live Daten (Referenz-Erlass EnG, SR 730.0).
2. **Erwartungs-Beweis.** Falltraps werden als Assertions eingelockt (FRBR-Richtung, leere CA-Felder, Task-Zwischenknoten). Bricht eine, hat sich das Fedlex-Datenmodell bewegt — der Test nennt den nötigen Lexikon-Update.
3. **Phantom-Wache.** Die Ausschlussliste wird aktiv überwacht. Beginnt Fedlex ein Phantom-Prädikat zu befüllen, schlägt der Audit-Test fehl und fordert ein neues Primitiv.

Seit 2026-06-10 sind **alle 27 Primitive in Rust implementiert**. Jeder Konformanz-Test
ruft die öffentliche Funktion **Ende-zu-Ende** gegen den Live-Endpoint auf. Falltrap-
und Phantom-Wachen laufen zusätzlich als Roh-Queries weiter.

**Lauf-Historie**

| Datum | Ergebnis | Befunde |
|---|---|---|
| 2026-06-10 | 29/29 pass (E2E) | Alle 27 Primitive in Rust implementiert, Suite von Pattern-Checks auf Ende-zu-Ende-Aufrufe umgestellt. Neue Befunde. (1) Die WAF blockt `SELECT DISTINCT` + `citationFromLegalResource` + URL-Literal (HTTP 400) — `get_citations` dedupliziert clientseitig. (2) EnG hat keine Annex-Subdivisions, Annex-Existenz wird systemweit geprüft. (3) Vernehmlassungs-Termine und Federführung liegen nur auf `hasSubTask`-Tasks. |
| 2026-06-10 | 29/29 pass (Pattern) | Erstlauf fand 4 Abweichungen. (1) Titel-Bug in `search_law`/`get_law_metadata` (Consolidation- statt CA-Expression) — **behoben**. (2) SR-Wiederverwendung (730.0 → 2 CAs) → RES-01 liefert Liste. (3) `descriptionFrom` inzwischen befüllt (J7.2 überholt). (4) `cogni:*` inzwischen öffentlich (J16.2 überholt). |

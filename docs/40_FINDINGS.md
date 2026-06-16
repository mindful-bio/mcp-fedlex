# 40 — Befunde: die Discovery-Lücke der MCP-Oberfläche

> **Was dieses Dokument ist.** Ein Befund-Protokoll. Es hält fest, *was heute im Code steht*,
> *welche Lücke daraus folgt* und *warum sie ansV blockiert*. Es trifft keine Entscheidung —
> die Entscheidung steht in [ADR-006](adr/ADR-006-discovery-tools-and-hint-provenance.md), die
> Umsetzung in [30_PLAN.md → M11](30_PLAN.md). Stand: 2026-06-16.

---

## F-1 — Die Daten-Schicht kann suchen, die Tool-Schicht nicht

Der JOLux-Funktionsraum (`10_LEXICON_jolux.md`) enthält in **Domäne 1 (RES)** und **Domäne 6
(TAX)** drei reine *Discovery*-Primitive — Operationen, die einen Erlass **finden**, ohne dass
der Aufrufer dessen ELI bereits kennt:

| Lexikon-ID | Funktion (`fedlex-jolux`) | Signatur | Rückgabe |
| --- | --- | --- | --- |
| JLX-RES-01 | `resolve_sr_number` | `(client, sr_number: &str, lang)` | `Vec<SrHit>` |
| JLX-RES-02 | `search_law` | `(client, query: &str, lang, limit)` | `Vec<LawHit>` |
| JLX-TAX-02 | `find_related_by_topic` | `(client, eli: &Eli, limit)` | `Vec<RelatedLaw>` |

Diese drei sind **implementiert, re-exportiert** (`fedlex_jolux::{search_law, resolve_sr_number,
find_related_by_topic}`) **und über die Live-Konformanzsuite abgedeckt**
(`crates/fedlex-jolux/tests/lexicon_conformance.rs`: `jlx_res_01`, `jlx_res_02`, `jlx_tax_02`;
alle 27 Konformanztests sind `#[ignore]` + live gegen `FEDLEX_SPARQL_ENDPOINT`).

**Aber:** Der Reader registriert in `register_navigation_tools` (`crates/mcp-reader/src/tools.rs`)
nur **acht** Tools, allesamt aus dem `LocalNavigation`-/`Validation`-Pool:

```
read_article · read_element · get_structure · search_text · get_metadata
get_references · get_modifications · compare_versions · read_document
```

Alle acht setzen den **ELI als Pflichtargument** voraus (`arg_eli`). **Kein einziges Tool
nimmt einen Titel, eine SR-Nummer oder ein Rechtsgebiet als Einstieg.** Die Such-Fähigkeit
existiert in der Bibliothek, ist aber an der MCP-Oberfläche nicht projiziert.

> **Kernbefund.** Die Lücke ist die **Tool-Projektion**, nicht die Daten-Schicht. Die Primitive
> sind fertig und getestet; es fehlt der dünne `McpTool`-Wrapper plus seine Registrierung.

---

## F-2 — Warum das ansV blockiert (der konkrete Schaden)

`ansV` spricht den Reader **generisch**: `ansv-fedlex` ruft `tools/list`, mappt jeden Eintrag
dynamisch auf eine OpenAI-Function-Definition (`crates/ansv-fedlex/src/llm.rs`) und reicht
`tools/call` durch. Das LLM sieht also **genau die Tools, die der Reader listet** — nicht mehr.

Die Verifikation in `ansv-fedlex/src/syllogism.rs` ist mechanisch: eine Quelle gilt nur als
`Verified`, wenn **ihr ELI in einem Evidence-Record des Laufs** vorkommt (ein protokollierter
`tools/call`). Alles andere ist `Unverified` — „möglicherweise halluziniert".

Daraus folgt die Blockade in einer Kette:

1. Eine reale Nutzerfrage nennt **kein ELI** („Gilt das Energiegesetz für …?"), sondern einen
   **Titel** oder eine **SR-Nummer**.
2. Das LLM hat **kein Tool**, um daraus einen ELI zu beschaffen — alle gelisteten Tools
   verlangen den ELI bereits als Eingabe.
3. Das LLM muss den ELI also **raten**. Ein geratener ELI erzeugt **keinen** Evidence-Record
   (oder einen zu falschem Erlass).
4. Die mechanische Verifikation stuft die Quelle korrekt als `Unverified` ein.

Das System ist damit **genau für den Normalfall** (Frage ohne ELI) nicht gutachtenfähig. Die
Auditierbarkeit, die ADR-004 strukturell garantiert, läuft am Einstiegspunkt leer.

---

## F-3 — Der Provenance-Konflikt (warum es kein triviales „Tool ergänzen" ist)

[ADR-004](adr/ADR-004-response-provenance.md) erzwingt **strukturell**, dass jede Tool-Antwort
eine `Provenance { eli, valid_as_of, transaction_time }` trägt: `McpTool::execute` gibt
ausschliesslich `Response<Value>` zurück, und `Response` ist ohne `Provenance` nicht
konstruierbar (privates Feld in `fedlex-core`). Akzeptanzkriterium dort: *„Eine Antwort ohne
auflösbares ELI ist ein Fehler, kein Sonderfall."* und *„jedes Listenelement trägt seine eigene
Provenance."*

Die Discovery-Primitive widersprechen dem **bewusst**. Ihre Docstrings sagen wörtlich:

- `search_law`: „**Discovery-Funktion ohne Provenance** — liefert Kandidaten, auf denen dann
  provenance-tragende Primitive (`get_law_metadata`, `get_article_text`) aufsetzen."
- `resolve_sr_number`: „Discovery-Funktion ohne Provenance."
- `find_related_by_topic`: „Discovery-Funktion ohne Provenance — liefert Kandidaten für RES-03."

Das ist **kein Versehen**, sondern fachlich richtig: Ein Suchtreffer ist eine **Hypothese**
(„dieser Erlass könnte gemeint sein"), keine **belegte Norm-Aussage**. Eine Norm-Provenance an
einen Suchtreffer zu hängen, würde genau die Trennung verwischen, die ADR-004 schützen will —
eine Aussage wäre dann „belegt", obwohl nur gesucht wurde.

> **Grenzziehung (für ADR-006 wichtig).** Nicht alle JOLux-Discovery-Funktionen sind
> provenance-los. `get_taxonomy` (JLX-TAX-01) liefert bereits `Response<Vec<TaxonomyEntry>>`
> **mit** Provenance, weil es eine Eigenschaft *eines bekannten* Erlasses ausliest. Nur die
> **drei** Funktionen aus F-1 (RES-01, RES-02, TAX-02) sind die echten provenance-losen
> Einstiegspunkte. Die Sonderregel betrifft genau diese drei.

---

## F-4 — RBAC: wohin gehören Discovery-Tools?

Die Pools (`crates/mcp-reader/src/tool.rs`) sind heute:

| Pool | Bedeutung | Rollen mit Zugriff |
| --- | --- | --- |
| `LocalNavigation` | Lokalen Cache lesen (AKN-Bäume) | Reader, Navigator, Validator |
| `LodFederation` | **Föderierte URI-Auflösung** | Navigator, Validator |
| `Workspace` | Stateful Scratchpad | Navigator, Validator |
| `Validation` | Schema-/Konsistenzprüfung, Diffing | Validator |

Discovery ist **Live-Auflösung gegen den Fedlex-Triplestore**, keine Lese-Operation auf dem
lokalen Cache. Fachlich gehört sie damit **nicht** in `LocalNavigation`, sondern zu
`LodFederation` (oder einen neuen, engeren `Discovery`-Pool). Konsequenz: Die nackte
`Reader`-Rolle sieht Discovery dann **nicht** — das ist eine bewusste Entscheidung, die in
ADR-006 zu treffen und gegen den ansV-Bedarf zu prüfen ist (ansV müsste mindestens mit der
`Navigator`-Rolle laufen).

---

## F-5 — Betriebsrisiko: ungewichtetes Quota auf Live-Calls

Das verteilte Quota (`crates/mcp-reader/src/quota.rs`, ADR-002) bucht pro Anfrage `cost` Tokens
aus einem claim-gebundenen Bucket. Der Transport-Pfad ruft `limiter.check(&claims, now_ms)`
**ohne tool-/pool-abhängiges Gewicht** — ein Discovery-Call (Live-SPARQL gegen
`fedlex.data.admin.ch`, langsam, externe Last) zählt heute **genauso viel wie** ein
Cache-Read. Das ist für die Lese-Tools unkritisch, wird aber mit Discovery zu einem
DoS-/Last-Vektor gegen den öffentlichen Fedlex-Endpoint.

> Folge für ADR-006: Discovery-Tools sollten ein **höheres Cost-Gewicht** tragen. Das ist eine
> kleine Erweiterung des Quota-Pfads (Gewicht aus `ToolPool`), kein Architektur-Umbau.

---

## Zusammenfassung der Befunde

| Befund | Aussage | Folge |
| --- | --- | --- |
| F-1 | 3 Discovery-Primitive fertig + getestet, aber nicht als MCP-Tools registriert | Wrapper + Registrierung fehlen |
| F-2 | ansV kann ohne Start-ELI keine Quelle mechanisch verifizieren | Normalfall (Frage ohne ELI) ist nicht gutachtenfähig |
| F-3 | Discovery ist bewusst provenance-los; ADR-004 fordert Provenance strukturell | Sonderregel nötig (Hinweis-Provenance) |
| F-4 | Discovery ist Live-Auflösung, nicht lokale Navigation | RBAC-Pool-Zuordnung zu klären (`LodFederation`/neuer Pool) |
| F-5 | Quota gewichtet Tools nicht; Live-SPARQL = externe Last | Höheres Cost-Gewicht für Discovery |

Die Entscheidung zu F-3/F-4/F-5 und der Tool-Schnitt stehen in
[ADR-006](adr/ADR-006-discovery-tools-and-hint-provenance.md); die Umsetzungsreihenfolge in
[30_PLAN.md → M11](30_PLAN.md).

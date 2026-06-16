# ADR-006 — Discovery-Tools & Hinweis-Provenance

- **Status:** Accepted (Plan)
- **Datum:** 2026-06-16
- **Betrifft:** Reader / Registry, RBAC-Pools, Quota, `fedlex-jolux`-Projektion, ansV-Konsum
- **Befund-Grundlage:** [40_FINDINGS.md](../40_FINDINGS.md) (F-1 bis F-5)
- **Verhältnis zu ADR-004:** **Ergänzung mit einer eng umrissenen Ausnahme.** ADR-004 bleibt
  die Regel; diese ADR definiert die einzige zulässige Abweichung und macht sie strukturell
  sichtbar.

## Kontext

Die MCP-Oberfläche des Readers projiziert heute nur **provenance-tragende Lese-/Navigations-
Primitive** (alle verlangen ein bekanntes ELI). Die drei **Discovery**-Primitive aus
`fedlex-jolux` — `search_law` (JLX-RES-02), `resolve_sr_number` (JLX-RES-01) und
`find_related_by_topic` (JLX-TAX-02) — sind implementiert und live-getestet, aber **nicht als
MCP-Tools registriert** (F-1). Damit kann ein Agent, der nur einen Titel oder eine SR-Nummer
kennt, **keinen Einstiegs-ELI** beschaffen; `ansV` kann den Normalfall „Frage ohne ELI" nicht
gutachtenfähig beantworten (F-2).

Der Grund, warum diese Tools nicht einfach „nachgereicht" wurden, ist ADR-004: Es erzwingt
strukturell, dass jede Antwort eine **Norm-Provenance** trägt. Discovery liefert aber bewusst
**keine** Norm-Provenance — ein Suchtreffer ist eine **Hypothese**, keine belegte Aussage (F-3).
Eine naive Lösung („Provenance halt irgendwie setzen") würde die Trennung zwischen *gefunden*
und *belegt* verwischen und damit den Kern von ADR-004 untergraben.

## Entscheidung

Wir registrieren die drei Discovery-Primitive als MCP-Tools (`search_law`, `resolve_sr_number`,
`find_related_topic`) und führen dafür eine **explizit benannte, strukturell sichtbare
Provenance-Variante** ein: die **Hinweis-Provenance** (`Provenance::Hint`).

### 1. Hinweis-Provenance statt Norm-Provenance

ADR-004 bleibt unverändert: `McpTool::execute` gibt weiterhin ausschliesslich `Response<Value>`
zurück, und `Response` ist ohne Provenance nicht konstruierbar. Discovery-Antworten verletzen
das Gate **nicht** — sie tragen eine Provenance, aber eine, die sich **als Hinweis ausweist**:

- Die Provenance eines Discovery-Treffers wird aus dem **Anfrage-Stempel** (`QueryStamp`, beide
  Zeitachsen) und dem **Treffer-ELI** gebildet — pro Treffer eine eigene (ADR-004
  „Listen/Aggregate"). Sie sagt: *„zum Stichtag X als Kandidat gefunden"*, nicht *„Norm Y
  besagt Z"*.
- Diese Provenance ist als `kind: "hint"` (vs. `kind: "norm"`) **maschinell unterscheidbar**.
  Der Konsument (`syllogismus-fedlex`/`ansV`) darf einen Hinweis **nicht** als Beleg zählen.
- Ein Treffer ohne ELI gibt es nicht (jeder `LawHit`/`SrHit`/`RelatedLaw` trägt ein ELI),
  daher bleibt das ADR-004-Kriterium „kein nacktes `T`, kein fehlendes ELI" gewahrt.

> Die Unterscheidung ist damit **strukturell**, nicht konventionell: Ein Konsument kann einen
> Hinweis nicht versehentlich als Norm-Beleg verbuchen, weil der Typ es ausweist.

### 2. RBAC: eigener `Discovery`-Pool

Discovery ist **Live-Auflösung gegen Fedlex**, keine lokale Navigation (F-4). Sie erhält einen
eigenen Pool `ToolPool::Discovery`. Sichtbarkeit:

- `Reader`: **kein** Discovery (nur lokaler Cache, Least-Privilege bleibt eng).
- `Navigator`: Discovery **ja** (das ist die Rolle, mit der `ansV` läuft).
- `Validator`: Discovery **ja**.

Ein eigener Pool (statt Einsortieren in `LodFederation`) hält die Bedeutung sauber: Föderation
löst *bekannte* kantonale/ausländische URIs auf; Discovery *findet* erst einen Bund-Erlass.

### 3. Quota: höheres Cost-Gewicht für Discovery

Discovery-Calls gehen live an den öffentlichen Fedlex-Endpoint (F-5). Der Quota-Pfad bekommt
ein **pool-abhängiges Cost-Gewicht**; `Discovery` wiegt schwerer als `LocalNavigation`. Das
schützt den externen Endpoint vor LLM-Schleifen und bleibt claim-gebunden (kein LLM-Parameter
kann das Gewicht senken, ADR-002).

### Akzeptanzkriterien (Code)

- [ ] **Drei Tools registriert.** `search_law`, `resolve_sr_number`, `find_related_topic` sind
      über `tools/list` (rollenabhängig) sichtbar und über `tools/call` aufrufbar.
- [ ] **Hinweis-Provenance existiert als Typ.** `fedlex-core` kennt `Provenance::Hint` (oder ein
      `kind`-Feld), das im Wire-Format als `"hint"` erscheint und sich von `"norm"` unterscheidet.
- [ ] **Pro Treffer eine Provenance.** Eine Discovery-Antwort mit n Treffern trägt n
      Hinweis-Provenances (kein Sammel-Slot), jede aus `QueryStamp` + Treffer-ELI.
- [ ] **RBAC.** `ToolPool::Discovery` existiert; `Reader` sieht die Tools **nicht**, `Navigator`
      und `Validator` sehen sie. Negativtest: `Reader`-`tools/call` auf `search_law` wird
      graceful abgewiesen.
- [ ] **Quota-Gewicht.** Ein Discovery-Call bucht mehr Tokens als ein `LocalNavigation`-Call;
      Test belegt das gewichtete Abbuchen.
- [ ] **Konsument-Vertrag.** Das `kind`-Feld ist Teil der Interface-Contracts mit
      `syllogismus-fedlex` (`fedlex-ecosystem/docs/20_INTERFACE_CONTRACTS.md`); ein Hinweis zählt
      dort nicht als Beleg.
- [ ] **Tests.** Dispatch-Test zeigt Hinweis-Provenance im Wire-Format; Test, dass ein
      Discovery-Treffer-ELI anschliessend von einem norm-tragenden Tool (`get_metadata`/
      `read_article`) aufgelöst werden kann (Discovery → Beleg-Kette).

## Begründung

- **Schliesst die ansV-Blockade an der Wurzel.** Der Agent kann aus Titel/SR-Nummer einen
  Einstiegs-ELI gewinnen und ihn dann mit norm-tragenden Tools belegen — die mechanische
  Verifikation in `ansv-fedlex` findet wieder ein ELI im Evidence-Log (F-2).
- **Wahrt ADR-004 strukturell.** Die Ausnahme ist typisiert und damit nicht versehentlich
  missbrauchbar; *gefunden* und *belegt* bleiben maschinell getrennt (F-3).
- **Minimaler Eingriff.** Die Primitive existieren; es entsteht nur die dünne Tool-Schicht plus
  ein Provenance-Variant und zwei kleine Querschnitt-Anpassungen (Pool, Quota-Gewicht).

## Alternativen

- **Norm-Provenance an Suchtreffer hängen.** Verworfen. Verwischt *gefunden* vs. *belegt* und
  höhlt ADR-004 aus.
- **Discovery in `LocalNavigation` für den Reader öffnen.** Verworfen. Falsche Semantik (kein
  Cache-Read) und unterläuft Least-Privilege; Live-Last würde der nackten Reader-Rolle offenstehen.
- **Discovery nur client-seitig in ansV (eigener SPARQL-Client).** Verworfen. Umgeht Quota,
  Provenance-Gate und Audit-Trail des Readers — genau die Garantien, die das System ausmachen.
- **Kein `kind`-Feld, nur Doku-Konvention.** Verworfen. ADR-004 hat bewusst auf strukturelle
  statt konventionelle Garantien gesetzt; eine Ausnahme per Kommentar wäre ein Rückschritt.

## Konsequenzen

- **Positiv.** Der Einstieg „Frage ohne ELI" wird gutachtenfähig; die Discovery→Beleg-Kette ist
  durchgängig und auditierbar; die Trennung Hinweis/Beleg ist typsicher.
- **Negativ.** Ein neuer Provenance-Variant berührt `fedlex-core` und alle Serialisierer;
  Konsumenten (`syllogismus-fedlex`, `ansV`) müssen das `kind`-Feld auswerten. Live-Discovery
  erhöht die Last gegen Fedlex (durch Quota-Gewicht gedämpft).
- **Modell.** Neuer `ToolPool::Discovery` und der Provenance-Variant `Hint` in der LikeC4-Spec;
  drei neue Tool-Knoten unter dem Reader.

---

## Status der Umsetzung
Architektur-Plan festgehalten. Implementierung offen — diese ADR ist die verbindliche
Akzeptanzkriterien-Liste für [30_PLAN.md → M11](../30_PLAN.md).

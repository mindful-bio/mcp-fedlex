# ADR-004: Response-Provenance-Envelope

- **Status:** Accepted (Plan / v6.1)
- **Datum:** 2026-06-01
- **Kontext-Artefakt:** `likec4/` (v6.1) — Komponenten `mcpRegistry.provenanceEnvelope`, `mcpRegistry.temporalResolver`
- **Betrifft:** `mcp-fedlex` (Reader / Navigator), nachgelagert `syllogismus-fedlex`

## Kontext

`mcp-fedlex` ist bi-temporal. Der `temporalResolver` stempelt jede **Anfrage** mit dem
gewünschten Zeitpunkt (`valid_as_of`) und löst die korrekte Norm-Version auf. Das adressiert
die Frage „welche Fassung galt?".

Es fehlte jedoch die symmetrische Garantie auf der **Antwort**-Seite. Eine Tool-Response trug
ihre Herkunft nur als Konvention, nicht strukturell. Damit fehlt die maschinell prüfbare
Rückführbarkeit jeder gelieferten Aussage auf `(eli, valid_as_of, transaction_time)`.

Das ist die wichtigste der hier behandelten Schwächen, denn `syllogismus-fedlex` baut darauf
einen **Audit-Trail** des Justizsyllogismus auf. Jede Prämisse (Obersatz/Norm) muss auf eine
exakte, zitierfähige Norm-Version zurückführbar sein. Ohne strukturelle Provenance in der
Antwort ist die Schlussfolgerung nicht auditierbar und juristisch wertlos.

## Entscheidung

Jede Tool-Antwort trägt **verpflichtend** eine Provenance-Hülle. Ein dediziertes Antwort-Gate
(`provenanceEnvelope`) erzwingt diese Invariante strukturell, sodass eine Antwort ohne
Provenance gar nicht erst den Server verlassen kann.

```
Response<T> {
  data: T,
  provenance: Provenance {
    eli: String,             // European Legislation Identifier der Quelle
    valid_as_of: Date,       // Gültigkeitszeit (welche Fassung galt)
    transaction_time: Date,  // Systemzeit (wann wurde sie erfasst)
  }
}
```

### Akzeptanzkriterien (Code)
- [ ] **Typsystem-Enforcement.** Tool-Antworten werden über einen generischen
      `Response<T>`-Wrapper geführt; ein nacktes `T` kann den Transport-Layer nicht
      passieren (Compile-Zeit-Garantie statt Konvention).
- [ ] **Pflichtfelder.** `eli`, `valid_as_of`, `transaction_time` sind nicht-optional. Eine
      Antwort ohne auflösbares ELI ist ein Fehler, kein Sonderfall.
- [ ] **Konsistenz mit dem Resolver.** Die `valid_as_of` der Antwort entspricht dem vom
      `temporalResolver` aufgelösten Zeitpunkt der Anfrage (kein Drift zwischen Frage und
      Beleg).
- [ ] **Listen/Aggregate.** Liefert ein Tool mehrere Quellen, trägt jedes Element seine
      eigene Provenance (keine Sammel-Provenance, die Herkunft verwischt).
- [ ] **Verträge.** Das Schema ist Teil der Interface-Contracts mit `syllogismus-fedlex`
      (siehe `fedlex-ecosystem/docs/20_INTERFACE_CONTRACTS.md`).
- [ ] **Tests.** Negativtest, der belegt, dass eine Tool-Implementierung ohne gesetzte
      Provenance nicht kompiliert bzw. zur Laufzeit hart fehlschlägt.

## Begründung

- **Auditierbarkeit ist die Kernfunktion.** Der nachgelagerte Reasoner ist nur so vertrauens-
  würdig wie die Rückführbarkeit seiner Prämissen. Provenance gehört deshalb in die Antwort,
  nicht in optionale Metadaten.
- **Strukturelle statt konventionelle Garantie.** Ein Gate plus Typsystem verhindert, dass
  eine einzelne Tool-Implementierung die Invariante versehentlich bricht.

## Alternativen

- **Provenance als optionales Metadatenfeld (v6.0).** Verworfen. Optional bedeutet in der
  Praxis „fehlt irgendwann", und genau dann bricht der Audit-Trail.
- **Provenance nur im Trace/Log.** Verworfen. Der Konsument (`syllogismus-fedlex`) braucht
  die Herkunft **in den Daten**, nicht in einem separaten Observability-Kanal.

## Konsequenzen

- **Positiv.** Jede Aussage ist maschinell auf eine Norm-Version rückführbar; der
  Audit-Trail des Reasoners ist lückenlos und zitierfähig.
- **Negativ.** Jede Tool-Antwort muss ihre Quelle führen; Tools ohne klares ELI (z.B. reine
  Aggregationen) müssen ihre Herkunft explizit zusammensetzen.
- **Modell.** Neue Komponente `provenanceEnvelope` (`#agentic`) in `mcpRegistry` als
  verpflichtendes Antwort-Gate.

---

## Status der Umsetzung
Architektur-Plan (LikeC4 v6.1) festgehalten. Implementierung offen — diese ADR ist die
verbindliche Akzeptanzkriterien-Liste für das spätere Coding.

# ADR-003: Index-Konsistenz (Embedding-Outbox) & Ingestion-Resilienz (DLQ)

- **Status:** Accepted (Plan / v6.1)
- **Datum:** 2026-06-01
- **Kontext-Artefakt:** `likec4/` (v6.1) — Store `ingestionSystem.embeddingOutbox`, Container `eventConsumer`, `indexWriter`, External `messageBroker`, `semanticService`
- **Betrifft:** `mcp-fedlex` (Writer / Ingestion-Seite)

## Kontext

Die Writer-Seite (CQRS-Schreibseite) verarbeitet New-Release-Events des ETL und
materialisiert den Korpus in drei Senken. Redis (DOM/Referenzen), Oxigraph (JOLux-Graph)
und `semantic-fedlex` (Vektor-Index). Zwei strukturelle Schwächen der v6.0-Modellierung
gefährden die Verlässlichkeit dieser Pipeline.

1. **Stiller Index-Drift.** Der Aufruf `indexWriter -> semanticService` (`index()`) war
   synchron und ohne Fallback modelliert. Fällt `semantic-fedlex` während der Ingestion
   aus (GPU-Dienst, eigene Verfügbarkeit), wird der Korpus geschrieben, aber **ohne
   Vektoren**. Das Ergebnis ist ein Korpus, dessen semantische Suche unvollständig ist,
   ohne dass jemand es bemerkt. Für ein Recherchewerkzeug ist eine stille Lücke schlimmer
   als ein sichtbarer Fehler.
2. **Poison-Message blockiert die Pipeline.** Der `eventConsumer` hatte keinen
   Dead-Letter-Pfad. Ein einziges fehlerhaftes XML-Release (Schema-Verstoss, Parser-Panic)
   kann den Konsum dauerhaft blockieren oder eine Endlos-Retry-Schleife erzeugen, die den
   gesamten Ingestion-Strom anhält.

## Entscheidung 1 — Embedding-Outbox statt synchronem index()-Aufruf

Der Embedding-Auftrag wird **transaktional in einer Outbox** (`embeddingOutbox`)
persistiert, nicht synchron an `semantic-fedlex` gepusht. Ein separater Zusteller liest
die Outbox und ruft `index()` mit Retry/Backoff auf.

### Akzeptanzkriterien (Code)
- [ ] **Transaktionale Outbox.** Der Embedding-Auftrag wird in derselben Transaktion wie
      der Korpus-Write festgeschrieben (kein Verlust zwischen Write und Enqueue).
- [ ] **Idempotenter Zusteller.** `index(eli, version, text)` ist idempotent pro
      `eli:version`, damit Retries keine Duplikate erzeugen.
- [ ] **Exponentielles Backoff** mit Obergrenze; dauerhaft scheiternde Aufträge wandern in
      einen Fehlerzustand mit Alarm an `legalEngineer`.
- [ ] **Vollständigkeits-Marker.** Pro `eli:version` ein Status `corpus_written` /
      `vectors_indexed`. Eine Abfrage „Korpus ohne Vektoren" macht Drift **sichtbar** und
      alarmierbar.
- [ ] **Metrik.** Outbox-Backlog-Tiefe und Zustell-Latenz als Span/Metrik.

## Entscheidung 2 — Dead-Letter-Queue für Poison-Releases

Der `eventConsumer` verschiebt nicht-verarbeitbare Releases nach begrenzten Retries in
eine **Dead-Letter-Queue** und blockiert den Strom nicht.

### Akzeptanzkriterien (Code)
- [ ] **Begrenzte Retries.** Nach N Versuchen (konfigurierbar) wandert die Nachricht in die
      DLQ, kein Endlos-Retry.
- [ ] **Kontext in der DLQ.** Fehlerursache, Release-ID, Versuchszähler und Rohnachricht
      werden mitgeführt, damit eine manuelle Re-Ingestion möglich ist.
- [ ] **Alarm.** DLQ-Zuwachs alarmiert `legalEngineer` über die Observability.
- [ ] **Re-Drive.** Ein bewusster Re-Ingestion-Pfad aus der DLQ existiert (nach Fix der
      Ursache).
- [ ] **Tests.** Ein bewusst fehlerhaftes Release blockiert nachfolgende valide Releases
      nicht.

## Begründung

- **Sichtbarkeit vor Verfügbarkeit.** Beide Mechanismen wandeln stille Datenfehler in
  sichtbare, alarmierbare Zustände. Für ein juristisches Recherchewerkzeug ist Korrektheit
  des Index nicht verhandelbar.
- **Entkopplung der Verfügbarkeiten.** Die Ingestion darf nicht an die Verfügbarkeit eines
  GPU-Dienstes gekoppelt sein. Die Outbox entkoppelt beide Lebenszyklen.

## Alternativen

- **Synchroner index()-Aufruf (v6.0).** Verworfen. Koppelt Verfügbarkeiten und erzeugt
  stillen Drift.
- **Re-Embedding per Nightly-Batch.** Als Ergänzung sinnvoll (Backfill), ersetzt aber die
  Outbox nicht, da die Drift-Fenster zu gross wären.
- **Synchrone Fehler-Propagation (Ingestion bricht ab).** Verworfen. Ein fehlendes
  Vektor-Backend würde die gesamte Korpus-Aktualisierung blockieren.

## Konsequenzen

- **Positiv.** Korpus und Vektor-Index konvergieren nachweisbar; eine einzelne fehlerhafte
  Quelle stoppt die Pipeline nicht.
- **Negativ.** Zusätzlicher Zustand (Outbox, DLQ) und ein Zusteller-Prozess. Das ist der
  Preis für eine eventual-konsistente, beobachtbare Schreibseite.
- **Modell.** Neuer Store `embeddingOutbox` (`#writer #state`); `embeddingOutbox -[http]->
  semanticService`; DLQ-Kante `eventConsumer -[event]-> messageBroker`.

---

## Status der Umsetzung
Architektur-Plan (LikeC4 v6.1) festgehalten. Implementierung offen — diese ADR ist die
verbindliche Akzeptanzkriterien-Liste für das spätere Coding.

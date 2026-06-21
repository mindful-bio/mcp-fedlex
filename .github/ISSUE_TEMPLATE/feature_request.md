---
name: "✨ Feature-Vorschlag"
about: "Eine neue Fähigkeit oder ein neues Tool vorschlagen."
title: "feat: <kurze Zusammenfassung>"
labels: ["enhancement", "needs-triage"]
assignees: []
---

> **Leitlinie:** Dieser Server projiziert bewusst **nur belegbaren Bedarf** auf Tools
> (siehe `docs/50_ROADMAP_TO_PERFECT.md §Nicht-Ziele` und `docs/45_GAP_ANALYSIS.md`).
> Vorschläge „auf Verdacht" werden eher abgelehnt als aufgenommen.

## Problem / Bedarf

<!-- Welches konkrete, belegbare Bedürfnis löst dieser Vorschlag? Wer hat es, wann? -->

## Vorgeschlagene Lösung

<!-- Was soll der Server tun? Bei einem neuen Tool: Name, Zweck, Ein-/Ausgabe. -->

## Einordnung in die Architektur

Bitte die relevanten Leitplanken berücksichtigen (siehe `CONTRIBUTING.md`):

- **Tool-Pool** (ADR-006/007): In welchen `ToolPool` fiele das? (LocalNavigation /
  Discovery / JoluxMetadata / …) — bestimmt das Quota-Gewicht.
- **Provenance** (ADR-004): Liefert die Antwort eine belegbare `eli` + `valid_as_of`,
  oder nur einen Hinweis (Discovery)?
- **Projektions-Matrix**: Ein neues Tool braucht einen Eintrag in
  `crates/mcp-reader/tests/lexicon_projection.rs`.
- **Datenquelle**: jolux / akn / bridge / extern?

## Alternativen / bereits geprüft

<!-- Bestehende Tools, die das teilweise abdecken? Warum reichen sie nicht? -->

## Checkliste

- [ ] Der Bedarf ist belegt (kein „auf Verdacht").
- [ ] Ich habe `45_GAP_ANALYSIS.md` und `50_ROADMAP_TO_PERFECT.md` auf Überschneidung geprüft.
- [ ] Der Vorschlag verletzt keine ADR-Invariante (Identität/Provenance/Least-Privilege).

# ADR-002: Verteiltes Quota & Rate-Limiting

- **Status:** Accepted (Plan / v6.1)
- **Datum:** 2026-06-01
- **Kontext-Artefakt:** `likec4/` (v6.1) — Komponente `mcp.transport.rateLimiter`, Store `sharedCache` (Redis)
- **Betrifft:** `mcp-fedlex` (Agentic Legal Navigator, MCP-Server)

---

## Kontext

Der MCP-Reader ist bewusst **zustandslos und horizontal skalierbar** (CQRS-Leseseite, mehrere Pods hinter einem Load-Balancer). Der `rateLimiter` ist Teil der Agent-Defense. Er drosselt Tool-Aufrufe pro SSE-Session und Tenant, um externe LOD-APIs vor Endlosschleifen zu schützen und LLM-Kosten zu begrenzen.

In der ursprünglichen Modellierung (v6.0) hielt der Token-Bucket seinen Zähler **pod-lokal**. Das ist im Widerspruch zur Zustandslosigkeit und erzeugt zwei konkrete Fehler.

1. **Limit skaliert mit der Pod-Zahl.** Bei N Reader-Pods ist das effektive Quota N-mal so hoch wie konfiguriert.
2. **Quota-Umgehung per Load-Balancing.** Ein Agent, dessen Requests über mehrere Pods verteilt werden, umgeht das Limit strukturell.

Für ein kostensensibles, missbrauchsexponiertes System (autonome LLM-Agenten) ist das ein reales Loch, kein theoretisches.

## Entscheidung

Der Token-Bucket-State wird **verteilt in Redis** (`sharedCache`) gehalten, nicht pod-lokal. Die Zähloperation ist atomar.

### Akzeptanzkriterien (Code)
- [ ] **Atomare Decrement-Operation** via Redis Lua-Script (Check-and-Decrement in einem Roundtrip), kein Read-Modify-Write über zwei Calls.
- [ ] **Schlüssel-Schema** `quota:{tenant_id}:{session_id}` mit `tenant_id` ausschliesslich aus dem server-validierten Claim (analog ADR-001, niemals aus LLM-Parametern).
- [ ] **Token-Bucket mit Refill-Rate** (nicht Fixed-Window), um Burst-Verhalten fair abzubilden.
- [ ] **Graceful Degradation.** Ist Redis kurz nicht erreichbar, gilt ein konservatives pod-lokales Fallback-Limit (fail-closed Richtung niedriger Durchsatz, nicht fail-open).
- [ ] **Metrik-Export.** Quota-Auslastung pro Tenant als Span/Metrik an die Observability (für `legalEngineer`).
- [ ] **Tests.** Nebenläufigkeits-Test, der belegt, dass das Gesamtlimit über mehrere simulierte Pods eingehalten wird.

## Begründung

- **Konsistenz mit CQRS.** Der einzige State eines „zustandslosen" Pods darf nicht pod-lokal sein. Redis ist bereits der verteilte L2-/Session-Store, also kein neues Infrastruktur-Element.
- **Atomarität schlägt Genauigkeitsverlust.** Ein Lua-Script verhindert Race-Conditions ohne verteiltes Locking.

## Alternativen

- **Pod-lokaler Bucket (v6.0).** Verworfen. Siehe Kontext, umgehbar und falsch skalierend.
- **Sticky Sessions am Load-Balancer.** Verworfen. Macht die Reader faktisch zustandsbehaftet und untergräbt horizontales Skalieren und Pod-Ausfallsicherheit.
- **Sidecar-Rate-Limiter (z.B. Envoy).** Tragfähig, aber verlagert die Tenant-/Session-Semantik aus der Anwendung. Zurückgestellt, da das Quota tenant-/rollenspezifisch und damit anwendungsnah ist.

## Konsequenzen

- **Positiv.** Korrektes Gesamtlimit, keine Umgehung, konsistent mit Zustandslosigkeit.
- **Negativ.** Ein Redis-Roundtrip pro Tool-Call auf dem Hot-Path. Mitigierbar durch lokales Token-Vorab-Leasing (kleine Batches) bei Bedarf.
- **Modell.** `rateLimiter` ist als `#state` getaggt und hat eine `data`-Kante zu `sharedCache`.

---

## Status der Umsetzung
Architektur-Plan (LikeC4 v6.1) festgehalten. Implementierung offen — diese ADR ist die
verbindliche Akzeptanzkriterien-Liste für das spätere Coding.

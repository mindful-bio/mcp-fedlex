# ADR-001: PII-Scrubbing & strikte Tenant-Isolation

- **Status:** Accepted (Plan / v4.0)
- **Datum:** 2026-05-30
- **Kontext-Artefakt:** `likec4/` (v4.0) — Komponenten `observabilityLayer.piiScrubber`, `mcpRegistry.poolWorkspace`, `l1Cache`
- **Betrifft:** `mcp-fedlex` (Agentic Legal Navigator, MCP-Server)

## Kontext

Der MCP-Server verarbeitet juristische Recherchen autonomer LLM-Agenten. Sobald ein
Agent einen **konkreten Fall** analysiert, fließen vertrauliche Mandantendaten (Namen,
Firmen, Aktenzeichen) durch Tool-Calls und Session-State. Daraus ergeben sich zwei
nicht verhandelbare Compliance-Leitplanken, die das Diagramm nur teilweise abbilden,
den **Code-Aufbau** aber maßgeblich bestimmen:

1. Traces/Spans dürfen keine PII in (ggf. Cloud-)Observability-Backends tragen.
2. In einem multi-mandantenfähigen System darf kein Tenant je den Workspace eines
   anderen lesen/schreiben — auch nicht durch eine LLM-Halluzination.

**Rechtlicher Rahmen:** Anwaltliches Berufsgeheimnis (Art. 321 StGB), revDSG/nDSG (CH),
DSGVO (EU). Ein Verstoß ist meldepflichtig und potenziell strafbar.

---

## Entscheidung 1 — PII-Scrubber als Vertrauensgrenze im Observability-Layer

Eine globale Redaction-Middleware (`observabilityLayer.piiScrubber`) maskiert PII,
**bevor** Spans/Metriken den Prozess Richtung OTLP-Backend verlassen.

### Akzeptanzkriterien (Code)
- [ ] **Allowlist statt Blocklist.** Nur explizit freigegebene Span-Attribute werden
      exportiert; alles andere ist per Default redacted. (Regex-Blocklisten für Namen
      sind unzulässig — sie lecken zuverlässig.)
- [ ] **Typsystem-Enforcement.** Sensible Felder werden als `Sensitive<T>`-Newtype
      (oder via `secrecy`-Crate) geführt; `Debug`/`Display` redacten. PII kann damit
      **nicht versehentlich** in ein `tracing::field` gelangen.
- [ ] **Redaction am Entstehungspunkt**, nicht erst im Exporter (eigener `tracing`-Layer).
      Der Export-seitige Scrub ist nur die letzte Verteidigungslinie.
- [ ] **Tests:** Property-/Snapshot-Tests, die sicherstellen, dass kein Roh-Tool-Argument
      und keine Roh-Response unmaskiert in einen exportierten Span gelangt.
- [ ] **Datenresidenz (ergänzend, nicht ersetzend):** Für die heikelsten Mandate
      self-hosted / EU-Region des Backends (Langfuse/Phoenix/Jaeger). Scrubbing ist
      Schicht 2, Residenz ist Schicht 1.

### Konsequenz
Diagramm-relevant: als Komponente `piiScrubber` (`#security`) im `observabilityLayer`
modelliert, da sie eine auditierbare Vertrauensgrenze bildet.

---

## Entscheidung 2 — Strikte Tenant-Isolation im Scratchpad (Redis-L2)

Der serverseitige Agent-Workspace (`mcpRegistry.poolWorkspace`, persistiert in
`sharedCache`/Redis) wird hart pro Tenant isoliert. Das ist **kein** Kryptografie-,
sondern ein **Vertrauens- und Enforcement-Problem**.

### Akzeptanzkriterien (Code)
- [ ] **`tenant_id` stammt ausschließlich aus dem server-validierten Claim**
      (`authRbac` / IdP), **niemals** aus einem Tool-/LLM-Parameter. (Sonst halluziniert
      sich Agent A die `tenant_id` von Kanzlei B zusammen.)
- [ ] **Enforcement im Data-Access-Layer, nicht per Konvention.** Ein Repository-Typ
      injiziert den Namespace `tenant_id:session_id:key` und ist die *einzige* Schreib-/
      Lese-Schnittstelle zum Scratchpad.
- [ ] **Key-Injection verhindern.** LLM-gelieferte Key-Bestandteile werden validiert/
      escaped; Separatoren (`:`) und Glob-Wildcards (`*`, `?`, `[`) werden abgelehnt
      (analog SQL-Injection → „Redis-Key-Injection").
- [ ] **Physische Defense-in-Depth.** Redis **ACL pro Tenant** mit Key-Pattern
      (`~tenant_id:*`) oder getrennte logische DBs — so kann selbst ein verbuggter Query
      fremde Keys nicht lesen.
- [ ] **L1-Falle (aus v4.0!).** Der moka-`l1Cache` ist **pro Pod über alle Sessions/
      Tenants geteilt**. Regel: **L1 enthält ausschließlich öffentliche, nicht-
      mandantenbezogene Daten** (geparster Gesetzeskorpus). Scratchpad-/Falldaten dürfen
      **nie** in L1 landen, sonst Cross-Tenant-Leak über die Hintertür.
- [ ] **TTL & Verschlüsselung.** Scratchpad-Einträge erhalten Expiry; falls echte
      Falldaten gespeichert werden, encryption-at-rest.
- [ ] **Tests:** Negativtests, die belegen, dass ein Zugriff mit fremder/erfundener
      `tenant_id` bzw. mit Ausbruchs-Keys hart fehlschlägt.

### Konsequenz
Code-Disziplin/Querschnitt — bewusst **nicht** als eigener Diagramm-Knoten modelliert
(der Workspace-Pool und L2/Redis existieren bereits). Die Isolation ist eine Invariante
des Data-Access-Layers.

---

## Status der Umsetzung
Architektur-Plan (LikeC4 v4.0) festgehalten. Implementierung offen — diese ADR ist die
verbindliche Akzeptanzkriterien-Liste für das spätere Coding.

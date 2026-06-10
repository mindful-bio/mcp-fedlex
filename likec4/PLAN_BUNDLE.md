# mcp-fedlex - Vollständiger LikeC4-Architektur-Plan

_Generiert am 2026-06-10 19:08:21 aus 3 Quelldateien:_ `specification.c4 model.c4 views.c4`

Föderierter "Agentic Legal Navigator" (MCP-Server, Rust/tokio). Der folgende
Block enthält die komplette `specification`, das `model` und alle `views`.

## specification.c4

```likec4
// ============================================================
// mcp-fedlex - Föderierter "Agentic Legal Navigator" (MCP-Server)
// specification.c4 (v7.0) - Element-, Tag- und Relationship-Arten
// ============================================================
specification {

  // ---------- Logische Element-Arten ----------
  element actor {
    notation 'Akteur (Agent / Mensch)'
    style {
      shape person
      color secondary
    }
  }

  element external {
    notation 'Externer Dienst / fremde Datenquelle / verteilter Store'
    style {
      shape cylinder
      color muted
    }
  }

  element system {
    notation 'System (eigenständig deploybar)'
    style {
      shape rectangle
    }
  }

  element container {
    notation 'Container (Rust-Modul / K8s-Workload)'
    style {
      shape rectangle
      color blue
    }
  }

  element component {
    notation 'Komponente (Rust-Submodul / Trait)'
    style {
      shape component
      color sky
    }
  }

  element store {
    notation 'In-Memory / Embedded Store'
    style {
      shape storage
      color indigo
    }
  }

  // ---------- Tags ----------
  tag mvp {
    color #2563eb
  }
  tag optional {
    color #6b7280
  }
  tag federation {
    color #d97706
  }
  tag security {
    color #dc2626
  }
  tag observability {
    color #16a34a
  }
  tag agentic {
    color #7c3aed
  }
  tag reader {
    color #0ea5e9
  }
  tag state {
    color #4f46e5
  }
  tag deferred {
    color #9ca3af
  }

  // ---------- Relationship-Arten ----------
  relationship mcptransport {
    notation 'MCP Streamable HTTP (JSON-RPC)'
  }
  relationship http {
    notation 'HTTP / SPARQL Connector'
    color amber
  }
  relationship data {
    notation 'Verteilter State-/Daten-Zugriff (Redis/Qdrant)'
    color indigo
  }
  relationship trace {
    notation 'OTel Span / Metrik Export'
    color green
    line dotted
  }
  relationship internal {
    notation 'Interner Async-Call (tokio)'
  }
}
```

## model.c4

```likec4
// ============================================================
// mcp-fedlex - Föderierter "Agentic Legal Navigator" (MCP-Server)
// model.c4 (v7.0) - Direct-Fetch Reader
// v7.0: Writer-Pfad entfernt (Indexierung lebt in semantic-fedlex),
//       fedlex-bridge als produktiver Direct-Fetch-Pfad (live verifiziert),
//       Redis/Oxigraph-Korpus-Stores als #deferred Skalierungs-Stufe,
//       Transport SSE -> Streamable HTTP (MCP-Spec-Wechsel)
// v6.1: verteiltes Quota, Provenance-Envelope, DLQ (ADR-002..004)
// ============================================================
model {

  // ========================================================
  // Akteure
  // ========================================================
  llmAgent = actor 'Autonomer LLM-Agent' {
    #agentic
    description 'Externer KI-Agent (z.B. Claude/GPT). Verbindet sich via Streamable HTTP, authentifiziert sich beim Connect und navigiert anschließend autonom durch den föderierten Rechts-Graphen - ausschließlich über MCP Tool-Calls.'
  }

  legalEngineer = actor 'Legal Engineer / SRE' {
    description 'Mensch im Betrieb. Überwacht Agenten-Loops, Payload-Sizes und Latenzen in den Observability-Dashboards und tuned RBAC-Rollen, Quotas & Tool-Pools.'
  }

  // ========================================================
  // Externe Systeme, verteilte Stores & Datenquellen
  // ========================================================
  fedlex = external 'Fedlex (Bund)' {
    #federation, #mvp
    description 'Die amtliche Quelle: SPARQL-Endpoint (JOLux-Graph, bi-temporal via dateApplicability) + Filestore mit immutablen AKN-XML-Manifestationen pro Konsolidierung/Sprache. Stabil betrieben vom Bund - der Reader holt direkt hier, ohne eigene Korpus-Kopie (live verifiziert: E2E-Kette in ~0.5s).'
    technology 'SPARQL / HTTPS (fedlex.data.admin.ch)'
  }

  sharedCache = external 'Distributed Cache (Redis)' {
    #state
    description 'Verteilter In-Memory-Store für Reader-State: atomarer Token-Bucket des Rate Limiters (Lua, pod-übergreifend, ADR-002) und serverseitiger Agent-Session-State (Scratchpad). Die frühere L2-Korpus-Cache-Rolle (rohe AKN-Bytes) ist eine deferred Skalierungs-Stufe - v1 holt Manifestationen direkt von Fedlex.'
    technology 'Redis'
  }

  semanticService = external 'semantic-fedlex (Embedding & Vector Search)' {
    #optional
    description 'Eigenständiger GPU-Dienst (eigenes Repo). Das hybride RAG-Add-on: kapselt Embedding-Modell, Vektor-DB UND die eigene Corpus-Ingestion (Fedlex -> Chunking -> index()). Der Reader ruft nur search(query, as_of, top_k) - Top-K-Treffer sind Wegweiser (Discovery), nie Quelle; die Rechtsaussage kommt immer über den strukturellen Pfad (Bridge -> AKN). Fällt er aus, navigiert mcp strukturell weiter.'
    technology 'gRPC / HTTP'
  }

  sharedGraphStore = external 'Distributed Graph Store (Oxigraph/RDF)' {
    #state, #federation, #deferred
    description 'DEFERRED (Skalierungs-Stufe): lokaler RDF-Triplestore als Replikat des JOLux-Graphen für deterministische Graph-Walks ohne externen Roundtrip. v1 braucht ihn nicht - die jolux-Primitive sprechen direkt mit dem stabilen Fedlex-SPARQL-Endpoint. Wird relevant bei Latenz-/Verfügbarkeits-Anforderungen oder Last-Schonung von Fedlex.'
    technology 'Oxigraph (SPARQL/RDF)'
  }

  idpVault = external 'Identity Provider & Secret Vault' {
    #security
    description 'Zentrale Identitäts- & Secret-Verwaltung (z.B. Keycloak / HashiCorp Vault). Validiert API-Keys und liefert die RBAC-Rollen, aus denen der zulässige Tool-Pool abgeleitet wird.'
    technology 'Keycloak / HashiCorp Vault'
  }

  observabilityStack = external 'Observability-Dashboards' {
    #observability
    description 'OTel-Backends für LLM-Observability: Langfuse, Arize Phoenix, Jaeger. Empfängt Traces, Spans und Metriken über OTLP.'
    technology 'OpenTelemetry / OTLP'
  }

  // Föderiertes LOD-Netzwerk als vernetzter globaler Graph (Linked Open Data)
  lodFederation = external 'Föderiertes LOD-Netzwerk' {
    #federation
    description 'Recht als vernetzter globaler Graph (LOD), aufgelöst über den European Legislation Identifier (ELI) als gemeinsames URI-Schema von Fedlex/Bund über die Kantone bis zur EU (plus ECLI für Rechtsprechung). Föderale Ebenen nach innen, fremde Rechtsgebiete nach außen.'

    cantonLod = external 'Kantonale LOD-Endpunkte' {
      description 'Föderale Ebene nach innen: kantonale Rechtssammlungen via eigene SPARQL/HTTP-Endpunkte.'
    }
    communeLod = external 'Kommunale LOD-Endpunkte' {
      description 'Gemeinde-Ebene: kommunale Erlasse, sofern als LOD publiziert.'
    }
    foreignLod = external 'EU / Internationale LOD' {
      description 'Fremde Rechtsgebiete nach außen: EUR-Lex (ELI), internationale Quellen.'
    }
  }

  // ========================================================
  // System: Agentic Legal Navigator (READER) - zustandslos
  // ========================================================
  mcp = system 'Agentic Legal Navigator (MCP-Server)' {
    #mvp, #reader
    description 'Zustandsloser, horizontal skalierbarer MCP-Reader. 100% Rust/tokio. Hält keinen eigenen Korpus - holt Recht direkt und stichtagsgenau von Fedlex (Direct Fetch via fedlex-bridge) und cached nur Immutables (Manifestationen) lokal.'
    technology 'Rust / tokio'

    // ----- Transport, Auth & Agent-Defense -----
    transport = container 'Streamable-HTTP-Transport & Auth-Gateway' {
      #security
      description 'MCP-Streamable-HTTP-Endpunkt (axum) als einzige Eingangstür - der ältere HTTP+SSE-Transport ist in der MCP-Spec deprecated. Terminiert Agenten-Verbindungen, authentifiziert, drosselt und leitet daraus die RBAC-Rolle ab, die an die MCP-Registry weitergeht.'
      technology 'Rust / axum / Streamable HTTP'

      streamHandler = component 'HTTP Session Handler' {
        description 'Hält MCP-Sessions pro Agent (Streamable HTTP, optional SSE-Antwort-Streams) und (de)serialisiert JSON-RPC MCP-Nachrichten.'
      }
      rateLimiter = component 'Rate Limiter & Quotas (verteilt)' {
        #security, #state
        description 'Agent-Defense: drosselt Tool-Aufrufe pro Session/Tenant, bevor sie zur Registry durchgereicht werden - schützt Fedlex & externe APIs vor Endlosschleifen und begrenzt Kosten. Der Token-Bucket-State liegt NICHT pod-lokal, sondern atomar in Redis (Lua-Script), sonst skaliert das Limit mit der Pod-Zahl hoch und ein Agent umgeht das Quota per Load-Balancing. Siehe ADR-002.'
      }
      authRbac = component 'Auth & RBAC-Resolver' {
        #security
        description 'Validiert Credentials beim Connect gegen den IdP/Vault und mappt den Agenten auf eine Rolle (z.B. reader / navigator / validator).'
      }
    }

    // ----- MCP-Registry (Tools + Resources + Prompts + Workspace) -----
    mcpRegistry = container 'MCP-Registry' {
      #security
      description 'Zentrale Registry für die volle MCP-Oberfläche: Tools (in RBAC-Pools inkl. Workspace), Resources und Prompts. tools/list liefert dynamisch NUR den zur Rolle passenden Pool (least privilege). Beherbergt die globale Graceful-Failure-Middleware.'
      technology 'Rust / trait McpTool'

      mcpToolTrait = component 'trait McpTool' {
        description 'Abstrahiert Tool-Ausführung und JSON-Schema-Generierung: jedes Tool implementiert execute() + schema().'
      }
      temporalResolver = component 'Temporal Resolver (Point-in-Time)' {
        #agentic
        description 'Bi-Temporalität: versieht jede Abfrage (per ELI) mit einem valid_as_of-Zeitstempel (plus transaction_time für rückwirkende Korrekturen) und mappt über die Bridge auf die ELI-konsolidierte Fassung des Stichtags statt blind der neuesten - juristisch zwingend. Fedlex trägt die Bi-Temporalität nativ (dateApplicability).'
      }
      toolDispatcher = component 'Dynamic Tool Dispatcher (Tool-RAG)' {
        #deferred
        description 'Skalierungs-getriggert (Bund + 26 Kantone + EU): lädt per Intent-Analyse nur die relevantesten Tools statt 50-200 Schemas. CAVEAT: viele MCP-Clients lesen die Tool-Liste statisch beim Connect (notifications/tools/list_changed wird ignoriert) - daher bevorzugt generische, parametrisierte Tools (z.B. query_canton_law(canton: CantonEnum, ...)) mit Least-Privilege in der Argument-Validierung statt dynamischem Ein-/Ausblenden. Bleibt #deferred.'
      }
      poolLocal = component 'Pool::LocalNavigation' {
        description 'Tools zur Navigation im Bundesrecht: Kompositionen der jolux-/akn-Primitive über die Bridge (Struktur, Artikeltext, Versionen, Zitate).'
      }
      poolLod = component 'Pool::LodFederation' {
        #federation
        description 'Tools für föderierte URI-Auflösung über Kantone, Gemeinden und Ausland.'
      }
      poolValidation = component 'Pool::Validation' {
        description 'Tools für Schema-/Konsistenz-Validierung und XML-Diffing.'
      }
      poolWorkspace = component 'Pool::Workspace (Scratchpad)' {
        #agentic
        description 'Stateful Workspace-Tools: Agenten legen Zwischenergebnisse, Notizen und LOD-Bookmarks serverseitig im Session-State (Redis) ab und entlasten so ihr Context-Window bei langen Recherchen.'
      }
      mcpResources = component 'MCP Resources' {
        description 'Statische, direkt lesbare Ressourcen (z.B. ELI-Referenzlisten/Kataloge, Schema-Definitionen), die der Agent ohne Tool-Aufruf über stabile ELI-URIs konsumieren kann.'
      }
      mcpPrompts = component 'MCP Prompts' {
        #agentic
        description 'Vordefinierte Agenten-Instruktionen/Workflows (z.B. "Versionsvergleich erstellen") als wiederverwendbare Prompt-Templates.'
      }
      gracefulFailure = component 'Graceful-Failure-Middleware' {
        #agentic
        description 'Globale Middleware: fängt ALLE Fehler (Netzwerk-Timeouts, GPU-Fehler, XML-Errors, Panics) ab und wandelt sie in lenkende JSON-Strings { error, hint } für das LLM um - niemals ein roher Crash ans LLM.'
      }
      provenanceEnvelope = component 'Provenance-Envelope (Antwort-Gate)' {
        #agentic
        description 'Erzwingt strukturell, dass JEDE Tool-Antwort ihre Herkunft trägt: Provenance { eli, valid_as_of, transaction_time }. Während der Temporal Resolver die Anfrage stempelt, garantiert dieses Gate die Rückführbarkeit der Antwort - die Grundlage für den Audit-Trail von syllogismus-fedlex (jede Aussage rückführbar auf Norm-ELI). Pflichtfeld, kein optionales Metadatum. Siehe ADR-004.'
      }
    }

    // ----- XML / AKN-Engine (Read-Path Heavy Lifting) -----
    xmlEngine = container 'XML / AKN-Engine' {
      #agentic
      description 'Native Rust-Verarbeitung der via Bridge beschafften AKN-DOM-Bäume (20 akn-Primitive). Makro-Tools erledigen Heavy-Lifting (z.B. XML-Diffing); das LLM erhält ausschließlich destilliertes Markdown.'
      technology 'Rust / fedlex-akn / roxmltree'

      macroTools = component 'Makro-Tool-Engine' {
        description 'Verdichtet teure Operationen (Version-Diff, Strukturanalyse) zu destilliertem Markdown statt Roh-XML.'
      }
      paginator = component 'Paginierungs-Guard' {
        #agentic
        description 'Schützt vor Context-Window-Overflow: limitierte Rückgaben inkl. Metadaten { has_more, next_page }.'
      }
      execSandbox = component 'Execution Sandbox & Timeout Guard' {
        #security
        description 'Schützt vor XPath/Regex-DoS halluzinierter Queries: führt jeden LLM-generierten Such-Query via spawn_blocking auf einem separaten Thread aus (NICHT im async-Reactor, sonst greift kein Timeout!), mit harter Wall-Clock-Deadline (~2s) und Linearzeit-Regex (rust regex + size_limit). Bei Abbruch Graceful-Failure: "Suchanfrage zu komplex, bitte präziser formulieren".'
      }
    }

    // ----- Federated URI-Resolver (LOD-Gateway) -----
    lodGateway = container 'Federated URI-Resolver (LOD-Gateway)' {
      #federation
      description 'Löst rechtliche Referenzen über ELI/ECLI auf und routet intelligent: Bundesrecht direkt via fedlex-bridge, föderale/fremde Quellen via HTTP-Connectoren an externe LOD-Endpunkte. ELI ist das gemeinsame Schema, das dieselbe Resolver-Logik von Bund über Kantone bis EU trägt. Herkunft wird dem Agenten abstrahiert.'
      technology 'Rust / reqwest'

      eliResolver = component 'ELI/ECLI Resolver' {
        #federation
        description 'Kern der Föderation: normalisiert ELI-Templates (Erlasse) und ECLI (Rechtsprechung) als gemeinsames URI-Schema über alle Ebenen (Bund/Kanton/EU), leitet daraus den Stichtag (konsolidierte Fassung) ab und entscheidet pro Referenz lokal-vs-extern.'
      }
      circuitBreaker = component 'Circuit Breaker' {
        #federation
        description 'Schützt vor kaskadierenden Ausfällen: blockt langsame/defekte externe Endpunkte sofort ab (open/half-open/closed) und verhindert das Aufstauen von tokio-Tasks.'
      }
      httpConnector = component 'HTTP-Connector-Pool' {
        #federation
        description 'Async HTTP/SPARQL-Clients zu kantonalen, kommunalen und ausländischen LOD-Endpunkten - nur über den Circuit Breaker erreichbar.'
      }
    }

    // ----- Fedlex-Bridge (Direct-Fetch-Pfad, produktiv) -----
    fedlexBridge = container 'Fedlex-Bridge (Direct Fetch)' {
      #mvp, #federation
      description 'Der produktive Beschaffungs-Pfad (crate fedlex-bridge, live verifiziert): HttpSparqlClient führt die 27 jolux-Primitive gegen den Fedlex-SPARQL-Endpoint aus, der AknFetcher komponiert Stichtags-Auflösung (JLX-TMP-02) -> XML-Download -> AKN-Parse (20 akn-Primitive). Provenance (ELI + valid_as_of + transaction_time) entsteht hier strukturell. Manifestations-URLs sind immutable - ideal cachebar.'
      technology 'Rust / fedlex-bridge / fedlex-jolux / fedlex-akn'
    }

    // ----- Semantik-Client (dünn, zu semantic-fedlex) -----
    semanticClient = container 'Semantic-Search-Client' {
      #optional
      description 'Dünner Client (KEINE GPU) zu semantic-fedlex. Reicht die Agenten-Query inkl. Stichtag an search() weiter und mappt die Top-K-Treffer zurück. Fällt der Dienst aus, degradiert mcp graceful auf rein strukturelle Navigation.'
      technology 'Rust / gRPC-Client'
    }

    // ----- L1 Cache (lokal, pro Reader-Pod) -----
    l1Cache = store 'L1 DOM-Cache (moka)' {
      #state
      description 'Pfeilschneller lokaler Cache: hält fertig geparste AKN-Dokumente im RAM, Schlüssel ist die Manifestations-URL (pro Konsolidierung/Sprache eindeutig und IMMUTABLE - keine Invalidierung nötig, neue Fassung = neue URL). Single-Flight via moka get_with gegen Cache-Stampede, RAM-Budget gegen OOM. Reiner, jederzeit aus Fedlex rekonstruierbarer Cache - Stateless gewahrt.'
      technology 'Rust / moka'
    }

    // ----- LLM Observability -----
    observabilityLayer = container 'Tracing- & OTel-Layer' {
      #observability
      description 'Lückenloses Tracing jeder Aktion via #[tracing::instrument]. Exportiert Spans/Metriken via tracing-opentelemetry. Misst Payload-Sizes, Latenzen (Rust vs. LLM-Thinking), Argumente/Antworten.'
      technology 'Rust / tracing / tracing-opentelemetry'

      piiScrubber = component 'PII-Scrubber (Compliance-Gate)' {
        #security
        description 'Vertrauensgrenze vor dem OTLP-Export: allowlist-basierte Redaction-Middleware, die Mandanten-PII (Namen/Firmen) am Entstehungspunkt via Sensitive<T>-Newtypes maskiert. Verhindert, dass vertrauliche Falldaten in Cloud-Backends (Langfuse/Datadog) gelangen - Berufsgeheimnis (Art. 321 StGB) / revDSG. Siehe ADR-001.'
      }
    }

    // ----- Interne Beziehungen (tokio async) -----
    transport -[internal]-> mcpRegistry 'gedrosselte, validierte Tool-Calls + RBAC-Rolle'
    mcpRegistry -[internal]-> xmlEngine 'führt lokale Navigations-/Validierungs-Tools aus'
    mcpRegistry -[internal]-> lodGateway 'führt Föderations-Tools aus'
    mcpRegistry -[internal]-> semanticClient 'optionale semantische Suche'
    xmlEngine -[internal]-> fedlexBridge 'fordert geparstes AKN-Dokument zum Stichtag an'
    lodGateway -[internal]-> fedlexBridge 'löst Bundesrecht-ELIs direkt auf (jolux-Primitive)'
    fedlexBridge -[internal]-> l1Cache 'cached geparste Dokumente (Key = Manifestations-URL)'

    // ----- Observability-Verkabelung (Cross-Cutting) -----
    transport -[trace]-> observabilityLayer 'Spans & Metriken'
    mcpRegistry -[trace]-> observabilityLayer 'Spans & Metriken'
    xmlEngine -[trace]-> observabilityLayer 'Spans & Metriken'
    lodGateway -[trace]-> observabilityLayer 'Spans & Metriken'
    fedlexBridge -[trace]-> observabilityLayer 'Fetch-Latenzen & Cache-Hit-Raten'
  }

  // ========================================================
  // System-übergreifende Beziehungen
  // ========================================================

  // -- Reader-Pfad (Agent -> MCP) --
  llmAgent -[mcptransport]-> transport 'verbindet, authentifiziert & ruft Tools (Streamable HTTP)'
  authRbac -[http]-> idpVault 'Validierung von API-Keys und Bezug von RBAC-Rollen'
  rateLimiter -[data]-> sharedCache 'verteilter Token-Bucket (atomar via Lua, pod-übergreifend)'

  // -- Direct Fetch (v1-Pfad, live verifiziert) --
  fedlexBridge -[http]-> fedlex 'SPARQL (jolux-Primitive) + XML-Download (Filestore)'

  // -- Verteilter Session-State & optionale Dienste --
  mcpRegistry -[data]-> sharedCache 'persistiert Agent-Session-Scratchpad (Workspace)'
  semanticClient -[http]-> semanticService 'search(query, as_of, top_k) - stichtagsgenaue Top-K-Suche (Discovery, nie Quelle)'
  lodGateway -[data]-> sharedGraphStore 'DEFERRED: lokale JOLux-Graph-Walks als Skalierungs-Stufe'

  // -- Föderierte Auflösung nach außen --
  lodGateway -[http]-> cantonLod 'proxt ELI-Auflösung'
  lodGateway -[http]-> communeLod 'proxt ELI-Auflösung'
  lodGateway -[http]-> foreignLod 'proxt ELI/CELEX-Auflösung (EUR-Lex)'

  // -- Observability (Reader + Mensch) --
  observabilityLayer -[trace]-> observabilityStack 'exportiert Traces & Metriken (OTLP)'
  legalEngineer -> observabilityStack 'überwacht Agenten-Loops, Quotas, Fetch-Latenzen & Fedlex-Verfügbarkeit'
}
```

## views.c4

```likec4
// ============================================================
// mcp-fedlex - Föderierter "Agentic Legal Navigator" (MCP-Server)
// views.c4 (v7.0) - Vision-, Direct-Fetch-, Plan- und Komponenten-Sichten
// ============================================================
views {

  // ========================================================
  // 1. VISION VIEW (Context / Big Picture)
  // ========================================================
  view vision {
    title '1 - Vision: Föderierter Agentic Legal Navigator (Context)'
    description 'Das große Bild. Autonome LLM-Agenten sprechen via Streamable HTTP mit dem zustandslosen MCP-Reader (Auth gegen IdP/Vault). Der Reader holt Bundesrecht direkt und stichtagsgenau von Fedlex (Direct Fetch via fedlex-bridge) - ohne eigene Korpus-Kopie. Die hybride RAG-Variante (Embedding/Vektor) lebt als optionales Add-on in semantic-fedlex. Föderale & internationale Rechtsgebiete werden dynamisch über das LOD-Netzwerk integriert, alles lückenlos in die Observability exportiert.'

    include
      llmAgent,
      legalEngineer,
      mcp,
      fedlex,
      sharedCache,
      semanticService,
      idpVault,
      lodFederation,
      lodFederation.cantonLod,
      lodFederation.communeLod,
      lodFederation.foreignLod,
      observabilityStack

    style * {
      opacity 75%
    }
    style lodFederation._ {
      color amber
    }
    style fedlex {
      color green
    }
  }

  // ========================================================
  // 2. DIRECT-FETCH VIEW (v1-Datenpfad & Skalierungs-Stufe)
  // ========================================================
  view directfetch {
    title '2 - Direct Fetch: Fedlex als Quelle, Stores als Skalierungs-Stufe'
    description 'Der v1-Datenpfad (live verifiziert): die fedlex-bridge löst per SPARQL die Stichtagsfassung auf, lädt die immutable XML-Manifestation und parst sie - gecached im lokalen moka-Store (Key = Manifestations-URL, keine Invalidierung nötig). Redis trägt nur Session-State & Quotas. Der Oxigraph-Korpus-Spiegel ist eine deferred Skalierungs-Stufe für Latenz-/Verfügbarkeits-Anforderungen.'

    include
      llmAgent,
      mcp,
      mcp.fedlexBridge,
      mcp.l1Cache,
      fedlex,
      sharedCache,
      sharedGraphStore

    style mcp {
      color sky
    }
    style fedlex {
      color green
    }
    style sharedGraphStore {
      opacity 40%
    }
  }

  // ========================================================
  // 3. PLAN VIEW (Container des READER / MCP)
  // ========================================================
  view plan of mcp {
    title '3 - Plan: MCP-Reader Architektur (Container)'
    description 'Zustandsloser Reader: Streamable-HTTP-Transport (Rate Limiter & Auth), MCP-Registry (Tools/Resources/Prompts/Workspace + Temporal Resolver, Tool-Dispatcher & globale Graceful-Failure-Middleware), XML/AKN-Engine (Execution Sandbox), Fedlex-Bridge (Direct Fetch, jolux+akn-Primitive), lokaler L1-DOM-Cache (moka), Federated URI-Resolver (Circuit Breaker), dünner Semantic-Search-Client (zu semantic-fedlex) und Observability-Layer. Session-State & Quotas liegen in Redis; der Korpus bleibt bei Fedlex.'

    include
      *,
      llmAgent,
      idpVault,
      fedlex,
      sharedCache,
      semanticService,
      lodFederation,
      observabilityStack

    style mcp.semanticClient {
      opacity 40%
    }
    style fedlex {
      color green
    }
  }

  // ========================================================
  // 5. Komponenten-Sichten (eine Ebene tiefer)
  // ========================================================

  // 5a - Transport: Agent-Defense (Rate Limiting) + Auth
  view component_transport of mcp.transport {
    title '5a - Edge: Rate Limiting, Auth & Identity'
    description 'Agent-Defense an der Eingangstür: der Rate Limiter drosselt Tool-Aufrufe pro Session/Tenant über einen verteilten Token-Bucket in Redis (atomar, pod-übergreifend, ADR-002), bevor sie weitergereicht werden; der Auth-Resolver validiert API-Keys gegen den IdP/Vault und bezieht die RBAC-Rolle.'
    include
      *,
      llmAgent,
      idpVault,
      mcp.mcpRegistry
  }

  // 5b - MCP-Registry: Tools (RBAC-Pools) + Workspace + Resources + Prompts + Temporal + Tool-RAG
  view component_mcp_registry of mcp.mcpRegistry {
    title '5b - MCP-Registry (Tools / Workspace / Temporal / Tool-RAG)'
    description 'Volle MCP-Oberfläche: trait McpTool + RBAC-Tool-Pools (LocalNavigation, LodFederation, Validation, Workspace-Scratchpad), Resources, Prompts, globale Graceful-Failure-Middleware, der Temporal Resolver (Point-in-Time / valid_as_of) und der skalierungs-getriggerte Dynamic Tool Dispatcher (Tool-RAG, deferred). Der Workspace-Pool persistiert Session-State in Redis.'
    include
      *,
      mcp.transport,
      mcp.xmlEngine,
      mcp.lodGateway,
      mcp.semanticClient,
      sharedCache
  }

  // 5c - ELI/ECLI-Resolver: Bridge für Bundesrecht + Circuit Breaking nach außen
  view component_lod_gateway of mcp.lodGateway {
    title '5c - ELI/ECLI-Resolver, Fedlex-Bridge & Circuit Breaking'
    description 'Der ELI/ECLI-Resolver nutzt ELI als gemeinsames URI-Schema (Bund/Kanton/EU) und routet pro Referenz: Bundesrecht direkt über die fedlex-bridge (jolux-Primitive), föderale/fremde Quellen über den HTTP-Connector-Pool. Der Circuit Breaker blockt defekte/langsame externe Endpunkte sofort ab. Der Oxigraph-Spiegel bleibt deferred.'
    include
      *,
      mcp.mcpRegistry,
      mcp.fedlexBridge,
      sharedGraphStore,
      lodFederation,
      lodFederation.cantonLod,
      lodFederation.communeLod,
      lodFederation.foreignLod

    style sharedGraphStore {
      opacity 40%
    }
  }

  // 5d - Agentic Design & Query-Sandboxing der XML/AKN-Engine
  view component_agentic_engine of mcp.xmlEngine {
    title '5d - XML/AKN-Engine: Makro-Tools, Pagination & Execution Sandbox'
    description 'LLM-Optimierung: Paginierung gegen Context-Overflow, Makro-Tools (XML-Diffing zu destilliertem Markdown) und die Execution Sandbox & Timeout Guard, die halluzinierte XPath/Regex-Queries via spawn_blocking + harter Deadline gegen DoS absichert. Beschafft AKN-Dokumente über die Fedlex-Bridge (gecached in moka).'
    include
      *,
      mcp.mcpRegistry,
      mcp.fedlexBridge,
      mcp.l1Cache
  }

  // 5g - Caching: immutable Manifestationen, kein Invalidierungs-Problem
  view component_caching {
    title '5g - Caching: Immutable Manifestationen (moka)'
    description 'Die Bridge cached geparste AKN-Dokumente in moka, Schlüssel ist die Manifestations-URL. Da Fedlex-Manifestationen immutable sind (neue Fassung = neue URL), entfällt das Invalidierungs-Problem strukturell - kein Broker, kein Event-Bus. Der Temporal Resolver garantiert, dass verschiedene Stichtage auf die korrekte Fassungs-URL auflösen. Redis hält nur Session-State & verteilte Quotas.'
    include
      mcp.xmlEngine,
      mcp.fedlexBridge,
      mcp.l1Cache,
      fedlex,
      sharedCache
  }

  // 5e - Semantic-Search-Client (zu semantic-fedlex)
  view component_semantic_client of mcp.semanticClient {
    title '5e - Semantic-Search-Client (zu semantic-fedlex)'
    description 'Dünner Client ohne GPU: reicht die Agenten-Query inkl. Stichtag an semantic-fedlex.search() weiter und mappt Top-K-Treffer zurück. Fällt der Dienst aus, degradiert mcp graceful auf strukturelle Navigation.'
    include
      *,
      mcp.mcpRegistry,
      semanticService
  }

  // 5f - Observability als Cross-Cutting Concern
  view component_observability of mcp.observabilityLayer {
    title '5f - Observability & PII-Scrubbing (Cross-Cutting)'
    description 'Alle Reader-Container (Transport, MCP-Registry, XML-Engine, LOD-Gateway, Fedlex-Bridge) senden Spans & Metriken an den OTel-Layer. Vor dem OTLP-Export an die externen Dashboards maskiert der PII-Scrubber Mandanten-Daten (Compliance-Gate, ADR-001).'
    include
      *,
      mcp.transport,
      mcp.mcpRegistry,
      mcp.xmlEngine,
      mcp.lodGateway,
      mcp.fedlexBridge,
      observabilityStack
  }
}
```


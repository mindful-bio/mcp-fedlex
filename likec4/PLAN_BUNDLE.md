# mcp-fedlex - Vollstaendiger LikeC4-Architektur-Plan

_Generiert am 2026-06-01 13:36:39 aus 3 Quelldateien:_ `specification.c4 model.c4 views.c4`

MCP Server for Fedlex. Der folgende Block enthaelt die komplette `specification`, das `model` und alle `views`.

## specification.c4

```likec4
// ============================================================
// mcp-fedlex - Föderierter "Agentic Legal Navigator" (MCP-Server)
// specification.c4 (v6.0) - Element-, Tag- und Relationship-Arten
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
  tag writer {
    color #db2777
  }
  tag state {
    color #4f46e5
  }
  tag deferred {
    color #9ca3af
  }

  // ---------- Relationship-Arten ----------
  relationship sse {
    notation 'HTTP Server-Sent Events (JSON-RPC)'
  }
  relationship http {
    notation 'HTTP / SPARQL Connector'
    color amber
  }
  relationship feed {
    notation 'Datenzufuhr (Korpus / RDF)'
  }
  relationship event {
    notation 'Event (Pub/Sub)'
    color indigo
    line dashed
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
// model.c4 (v6.1) - CQRS / K8s & Day-2 Operations Readiness
// v6.1: verteiltes Quota, Provenance-Envelope, Embedding-Outbox, DLQ (ADR-002..004)
// ============================================================
model {

  // ========================================================
  // Akteure
  // ========================================================
  llmAgent = actor 'Autonomer LLM-Agent' {
    #agentic
    description 'Externer KI-Agent (z.B. Claude/GPT). Baut SSE-Verbindung auf, authentifiziert sich beim Connect und navigiert anschließend autonom durch den föderierten Rechts-Graphen - ausschließlich über MCP Tool-Calls.'
  }

  legalEngineer = actor 'Legal Engineer / SRE' {
    description 'Mensch im Betrieb. Überwacht Agenten-Loops, Payload-Sizes und Latenzen in den Observability-Dashboards und tuned RBAC-Rollen, Quotas & Tool-Pools.'
  }

  // ========================================================
  // Externe Systeme, Broker, verteilte Stores & Datenquellen
  // ========================================================
  etl = external 'Lokale ETL-Pipeline' {
    #federation
    description 'Interne Pipeline. Normalisiert Roh-Rechtsdaten (Fedlex) nach Akoma Ntoso (AKN) / Jolux - durchgängig über ELI identifiziert (Fedlex publiziert CH-Bundesrecht ELI-konform). Versioniert XML-Korpora + RDF-Tripel und publiziert bei jedem neuen Release ein Event an den Broker.'
    technology 'AKN / Jolux / RDF'
  }

  messageBroker = external 'Event Broker (Kafka/NATS)' {
    #state
    description 'Entkoppelt Producer (ETL) und Consumer (Ingestion). Trägt "New-Release"-Events und ersetzt ineffizientes Polling durch Push-Semantik.'
    technology 'Apache Kafka / NATS JetStream'
  }

  sharedCache = external 'Distributed Cache (Redis)' {
    #state
    description 'Verteilter In-Memory-Store & L2-Cache. Hält rohe AKN-Bytes/Referenzen bi-temporal versioniert (Key = ELI + valid_as_of) sowie serverseitigen Agent-Session-State (Scratchpad). Löst L1-Misses der Reader auf und ermöglicht zustandsloses, horizontales Skalieren.'
    technology 'Redis Cluster'
  }

  semanticService = external 'semantic-fedlex (Embedding & Vector Search)' {
    #optional
    description 'Eigenständiger GPU-Dienst (eigenes Repo). Kapselt Embedding-Modell UND Vektor-DB: index(text, eli, valid_as_of) für den Writer, search(query, as_of, top_k) für den Reader. Garantiert Index/Query-Modell-Konsistenz. Optional - fällt er aus, navigiert mcp strukturell (Cache/Graph) weiter.'
    technology 'gRPC / HTTP'
  }

  sharedGraphStore = external 'Distributed Graph Store (Oxigraph/RDF)' {
    #state, #federation
    description 'RDF-Triplestore mit dem vom Writer extrahierten JOLux-Graphen. Ermöglicht lokale, deterministische Graph-Walks (z.B. "welche Verordnung stützt sich auf dieses Bundesgesetz?") via SPARQL - ohne teuren externen Roundtrip. Schließt die Graph-Lücke des reinen KV-L2 und macht die LOD-Vision lokal performant.'
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
  // System: Legal Data Ingestion (WRITER) - CQRS Schreibpfad
  // ========================================================
  ingestionSystem = system 'Legal Data Ingestion (Writer)' {
    #writer, #federation
    description 'Eigenständig deploybarer Writer (CQRS-Schreibseite). Event-getrieben: konsumiert New-Release-Events, parst XML-Releases und befüllt die verteilten Stores. Skaliert unabhängig vom Reader.'
    technology 'Rust / tokio'

    eventConsumer = container 'Event Consumer' {
      #writer
      description 'Subscribed den Event-Broker, dedupliziert und triggert pro New-Release-Event eine asynchrone Ingestion-Pipeline.'
      technology 'Rust / rdkafka | async-nats'
    }
    ingestParser = container 'AKN/Jolux Parser (Writer)' {
      #writer
      description 'Lädt das neue XML-Release aus der ETL und parst gigabytegroße AKN/Jolux-Dokumente nativ in DOM-Bäume & Graph-Knoten.'
      technology 'Rust / quick-xml / roxmltree'
    }
    indexWriter = container 'Cache & Graph Indexer' {
      #writer, #state
      description 'Schreibt geparste DOM-Bäume/Referenzen nach Redis und den extrahierten JOLux-Graphen (RDF-Triples) nach Oxigraph - bi-temporal & append-only (jede Version bleibt erhalten, valid_as_of/transaction_time pro Knoten). Embedding + Vektor-Indexierung delegiert an semantic-fedlex (index-API) - entkoppelt über die Embedding-Outbox. Publiziert nach jedem Write ein Cache-Invalidierungs-Event an den Broker.'
      technology 'Rust / tokio'
    }

    embeddingOutbox = store 'Embedding-Outbox' {
      #writer, #state
      description 'Transaktionale Outbox für index()-Aufrufe an semantic-fedlex. Entkoppelt den synchronen Korpus-Write vom GPU-Dienst: fällt semantic-fedlex während der Ingestion aus, bleibt der Embedding-Auftrag persistent in der Outbox und wird mit Backoff erneut zugestellt. Ein Vollständigkeits-Marker pro ELI/Version macht Index-Drift (Korpus ohne Vektoren) sichtbar statt still. Siehe ADR-003.'
      technology 'Rust / Outbox-Pattern'
    }

    // ----- Ingestion-interne Pipeline -----
    eventConsumer -[internal]-> ingestParser 'triggert Parsing pro Release'
    ingestParser -[internal]-> indexWriter 'übergibt geparste DOM-Bäume & extrahierte Knoten'
    indexWriter -[internal]-> embeddingOutbox 'reiht Embedding-Aufträge ein (transaktional)'
  }

  // ========================================================
  // System: Agentic Legal Navigator (READER) - zustandslos
  // ========================================================
  mcp = system 'Agentic Legal Navigator (MCP-Server)' {
    #mvp, #reader
    description 'Zustandsloser, horizontal skalierbarer MCP-Reader (CQRS-Leseseite). 100% Rust/tokio. Hält keinen lokalen State - liest ausschließlich aus den verteilten Stores.'
    technology 'Rust / tokio'

    // ----- Transport, Auth & Agent-Defense -----
    transport = container 'SSE-Transport & Auth-Gateway' {
      #security
      description 'HTTP-SSE-Endpunkt (axum) als einzige Eingangstür. Terminiert Agenten-Verbindungen, authentifiziert, drosselt und leitet daraus die RBAC-Rolle ab, die an die MCP-Registry weitergeht.'
      technology 'Rust / axum / SSE'

      sseHandler = component 'SSE Session Handler' {
        description 'Hält langlebige SSE-Streams pro Agent und (de)serialisiert JSON-RPC MCP-Nachrichten.'
      }
      rateLimiter = component 'Rate Limiter & Quotas (verteilt)' {
        #security, #state
        description 'Agent-Defense: drosselt Tool-Aufrufe pro SSE-Session/Tenant, bevor sie zur Registry durchgereicht werden - schützt externe APIs vor Endlosschleifen und begrenzt Kosten. Der Token-Bucket-State liegt NICHT pod-lokal, sondern atomar in Redis (Lua-Script), sonst skaliert das Limit mit der Pod-Zahl hoch und ein Agent umgeht das Quota per Load-Balancing. Siehe ADR-002.'
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
        description 'Bi-Temporalität: versieht jede Abfrage (per ELI) an Cache/Graph/Vector-DB mit einem valid_as_of-Zeitstempel (plus transaction_time für rückwirkende Korrekturen) und mappt auf die ELI-konsolidierte Fassung des Stichtags statt blind der neuesten - juristisch zwingend.'
      }
      toolDispatcher = component 'Dynamic Tool Dispatcher (Tool-RAG)' {
        #deferred
        description 'Skalierungs-getriggert (Bund + 26 Kantone + EU): lädt per Intent-Analyse nur die relevantesten Tools statt 50-200 Schemas. CAVEAT: viele MCP-Clients lesen die Tool-Liste statisch beim Connect (notifications/tools/list_changed wird ignoriert) - daher bevorzugt generische, parametrisierte Tools (z.B. query_canton_law(canton: CantonEnum, ...)) mit Least-Privilege in der Argument-Validierung statt dynamischem Ein-/Ausblenden. Bleibt #deferred.'
      }
      poolLocal = component 'Pool::LocalNavigation' {
        description 'Tools zur Navigation im Cache (AKN-Bäume durchlaufen, Knoten/Artikel lesen).'
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
      description 'Native Rust-Verarbeitung der im Cache liegenden AKN/Jolux-DOM-Bäume. Makro-Tools erledigen Heavy-Lifting (z.B. XML-Diffing); das LLM erhält ausschließlich destilliertes Markdown.'
      technology 'Rust / quick-xml / roxmltree'

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
      description 'Löst rechtliche Referenzen über ELI/ECLI auf und routet intelligent: lokale JOLux-Graph-Walks im RDF-Store / Distributed Cache ODER Proxy via HTTP-Connectoren an externe LOD-Endpunkte. ELI ist das gemeinsame Schema, das dieselbe Resolver-Logik von Bund über Kantone bis EU trägt; nur föderale/fremde Quellen gehen nach außen. Herkunft wird dem Agenten abstrahiert.'
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

    // ----- Semantik-Client (dünn, zu semantic-fedlex) -----
    semanticClient = container 'Semantic-Search-Client' {
      #optional
      description 'Dünner Client (KEINE GPU) zu semantic-fedlex. Reicht die Agenten-Query inkl. Stichtag an search() weiter und mappt die Top-K-Treffer zurück. Fällt der Dienst aus, degradiert mcp graceful auf rein strukturelle Navigation.'
      technology 'Rust / gRPC-Client'
    }

    // ----- L1 Cache (lokal, pro Reader-Pod) -----
    l1Cache = store 'L1 DOM-Cache (moka)' {
      #state
      description 'Pfeilschneller lokaler L1: hält fertig geparste Rust-DOM-Bäume artikel-/abschnittsgranular (nicht ganze 50-MB-Gesetze) im RAM. Single-Flight via moka get_with (ein Parse pro Key/Pod -> kein Cache-Stampede). Weigher nach echter Allokationsgröße mit absolutem RAM-Budget gegen OOM. Reiner, aus L2 rekonstruierbarer Cache (Stateless gewahrt); Invalidierung sofort via Broker-Event, Re-Population koaliert.'
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
    xmlEngine -[internal]-> l1Cache 'liest fertig geparsten Rust-DOM (L1-Hit)'

    // ----- Observability-Verkabelung (Cross-Cutting) -----
    transport -[trace]-> observabilityLayer 'Spans & Metriken'
    mcpRegistry -[trace]-> observabilityLayer 'Spans & Metriken'
    xmlEngine -[trace]-> observabilityLayer 'Spans & Metriken'
    lodGateway -[trace]-> observabilityLayer 'Spans & Metriken'
  }

  // ========================================================
  // System-übergreifende Beziehungen
  // ========================================================

  // -- Reader-Pfad (Agent -> MCP) --
  llmAgent -[sse]-> transport 'verbindet, authentifiziert & ruft Tools'
  authRbac -[http]-> idpVault 'Validierung von API-Keys und Bezug von RBAC-Rollen'
  rateLimiter -[data]-> sharedCache 'verteilter Token-Bucket (atomar via Lua, pod-übergreifend)'

  // -- Reader: Two-Tier Caching (L1 lokal -> L2 Redis) & verteilter State --
  l1Cache -[data]-> sharedCache 'L2-Fallback: lädt rohe AKN-Bytes bei L1-Miss'
  l1Cache -[event]-> messageBroker 'abonniert Cache-Invalidierung (evict bei neuer Version)'
  lodGateway -[data]-> sharedCache 'liest/cached aufgelöste LOD-Referenzen'
  mcpRegistry -[data]-> sharedCache 'persistiert Agent-Session-Scratchpad (Workspace)'
  semanticClient -[http]-> semanticService 'search(query, as_of, top_k) - stichtagsgenaue Top-K-Suche'
  lodGateway -[data]-> sharedGraphStore 'lokale JOLux-Graph-Walks (SPARQL, deterministisch)'

  // -- Föderierte Auflösung nach außen --
  lodGateway -[http]-> cantonLod 'proxt ELI-Auflösung'
  lodGateway -[http]-> communeLod 'proxt ELI-Auflösung'
  lodGateway -[http]-> foreignLod 'proxt ELI/CELEX-Auflösung (EUR-Lex)'

  // -- Event-Driven Writer-Pfad (Push statt Poll) --
  etl -[event]-> messageBroker 'publiziert New-Release-Events'
  eventConsumer -[event]-> messageBroker 'konsumiert New-Release-Events (subscribe)'
  eventConsumer -[event]-> messageBroker 'verschiebt Poison-Releases in die Dead-Letter-Queue (kein Endlos-Retry)'
  ingestParser -[feed]-> etl 'lädt neues XML-Release'
  indexWriter -[data]-> sharedCache 'schreibt DOM-Bäume & Referenzen (append-only)'
  embeddingOutbox -[http]-> semanticService 'index(text, eli, valid_as_of) - mit Retry/Backoff, Embedding+Vektor ausgelagert'
  indexWriter -[data]-> sharedGraphStore 'materialisiert extrahierten JOLux-Graph (RDF-Triples)'
  indexWriter -[event]-> messageBroker 'publiziert Cache-Invalidierungs-Events'

  // -- Observability (Reader + Writer + Mensch) --
  observabilityLayer -[trace]-> observabilityStack 'exportiert Traces & Metriken (OTLP)'
  eventConsumer -[trace]-> observabilityStack 'Ingestion-Spans & Lag-Metriken'
  indexWriter -[trace]-> observabilityStack 'Write-Throughput & Latenzen'
  legalEngineer -> observabilityStack 'überwacht Agenten-Loops, Quotas, Ingestion-Lag, DLQ & Embedding-Backlog'
}
```

## views.c4

```likec4
// ============================================================
// mcp-fedlex - Föderierter "Agentic Legal Navigator" (MCP-Server)
// views.c4 (v6.0) - Vision-, CQRS-, Plan- und Komponenten-Sichten
// ============================================================
views {

  // ========================================================
  // 1. VISION VIEW (Context / Big Picture)
  // ========================================================
  view vision {
    title '1 - Vision: Föderierter Agentic Legal Navigator (Context)'
    description 'Das große Bild. Autonome LLM-Agenten sprechen via SSE mit dem zustandslosen MCP-Reader (Auth gegen IdP/Vault). Ein eigenständiges, event-getriebenes Ingestion-System (Writer) konsumiert New-Release-Events vom Broker und befüllt die verteilten Stores (Redis/Qdrant), aus denen der Reader liest. Föderale & internationale Rechtsgebiete werden dynamisch über das LOD-Netzwerk integriert, alles lückenlos in die Observability exportiert.'

    include
      llmAgent,
      legalEngineer,
      mcp,
      ingestionSystem,
      etl,
      messageBroker,
      sharedCache,
      semanticService,
      sharedGraphStore,
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
  }

  // ========================================================
  // 2. CQRS VIEW (Reader/Writer-Trennung & verteilter State)
  // ========================================================
  view cqrs {
    title '2 - CQRS: Reader/Writer-Trennung & verteilter State'
    description 'Der Schreibpfad (Ingestion, Writer) ist vom Lesepfad (MCP, Reader) entkoppelt: ETL pusht Events über den Broker an den Writer, der Redis (L2) und den Oxigraph-Graph befüllt und Embedding/Vektor an semantic-fedlex delegiert. Der zustandslose Reader skaliert horizontal in Kubernetes und liest nur aus den geteilten Stores.'

    include
      etl,
      messageBroker,
      ingestionSystem,
      sharedCache,
      semanticService,
      sharedGraphStore,
      mcp,
      llmAgent

    style mcp {
      color sky
    }
    style ingestionSystem {
      color red
    }
    style sharedCache, sharedGraphStore, messageBroker {
      color indigo
    }
  }

  // ========================================================
  // 3. PLAN VIEW (Container des READER / MCP)
  // ========================================================
  view plan of mcp {
    title '3 - Plan: MCP-Reader Architektur (Container)'
    description 'Zustandsloser Reader: SSE-Transport (Rate Limiter & Auth), MCP-Registry (Tools/Resources/Prompts/Workspace + Temporal Resolver, Tool-Dispatcher & globale Graceful-Failure-Middleware), XML/AKN-Engine (Execution Sandbox), lokaler L1-DOM-Cache (moka), Federated URI-Resolver (Circuit Breaker), dünner Semantic-Search-Client (zu semantic-fedlex) und Observability-Layer. Persistenter State liegt extern in Redis (L2) + Oxigraph-Graph; Embedding/Vektor via semantic-fedlex.'

    include
      *,
      llmAgent,
      idpVault,
      sharedCache,
      semanticService,
      sharedGraphStore,
      messageBroker,
      lodFederation,
      observabilityStack

    style mcp.semanticClient {
      opacity 40%
    }
  }

  // ========================================================
  // 4. INGESTION PLAN VIEW (Container des WRITER)
  // ========================================================
  view ingestion_plan of ingestionSystem {
    title '4 - Plan: Ingestion-Writer Architektur (Container)'
    description 'Event-getriebener Schreibpfad: Event Consumer subscribed den Broker, der Parser lädt & verarbeitet das XML-Release aus der ETL, der Indexer schreibt idempotent nach Redis (DOM/Referenzen) und Oxigraph (JOLux-Graph) und delegiert Embedding+Vektor-Indexierung an semantic-fedlex (index-API).'

    include
      *,
      etl,
      messageBroker,
      sharedCache,
      semanticService,
      sharedGraphStore,
      observabilityStack
  }

  // ========================================================
  // 5. Komponenten-Sichten (eine Ebene tiefer)
  // ========================================================

  // 5a - Transport: Agent-Defense (Rate Limiting) + Auth
  view component_transport of mcp.transport {
    title '5a - Edge: Rate Limiting, Auth & Identity'
    description 'Agent-Defense an der Eingangstür: der Rate Limiter drosselt Tool-Aufrufe pro Session/Tenant (Token-Bucket), bevor sie weitergereicht werden; der Auth-Resolver validiert API-Keys gegen den IdP/Vault und bezieht die RBAC-Rolle.'
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

  // 5c - ELI/ECLI-Resolver: lokale Graph-Walks + Circuit Breaking
  view component_lod_gateway of mcp.lodGateway {
    title '5c - ELI/ECLI-Resolver, JOLux-Graph & Circuit Breaking'
    description 'Der ELI/ECLI-Resolver nutzt ELI als gemeinsames URI-Schema (Bund/Kanton/EU) und entscheidet zwischen lokalem JOLux-Graph-Store (SPARQL-Walks, deterministisch), Distributed Cache und externem Proxy. Der Circuit Breaker blockt defekte/langsame Endpunkte sofort ab, bevor der HTTP-Connector-Pool externe LOD-Quellen anspricht.'
    include
      *,
      mcp.mcpRegistry,
      sharedCache,
      sharedGraphStore,
      lodFederation,
      lodFederation.cantonLod,
      lodFederation.communeLod,
      lodFederation.foreignLod
  }

  // 5d - Agentic Design & Query-Sandboxing der XML/AKN-Engine
  view component_agentic_engine of mcp.xmlEngine {
    title '5d - XML/AKN-Engine: Makro-Tools, Pagination & Execution Sandbox'
    description 'LLM-Optimierung: Paginierung gegen Context-Overflow, Makro-Tools (XML-Diffing zu destilliertem Markdown) und die Execution Sandbox & Timeout Guard, die halluzinierte XPath/Regex-Queries via spawn_blocking + harter Deadline gegen DoS absichert. Liest fertig geparste Bäume aus dem lokalen L1-Cache (Miss -> L2/Redis).'
    include
      *,
      mcp.mcpRegistry,
      mcp.l1Cache,
      sharedCache
  }

  // 5g - Two-Tier Caching (L1/L2) & Broker-Invalidierung
  view component_caching {
    title '5g - Two-Tier Caching (L1/L2) & Invalidierung'
    description 'L1 (moka, pro Reader-Pod) hält fertig geparste Rust-DOM-Bäume; bei Miss lädt L1 rohe AKN-Bytes aus L2 (Redis). Schreibt der Writer eine neue bi-temporale Version, publiziert er ein Invalidierungs-Event über den Broker, das die L1-Caches aller Reader gezielt evictet - so bleibt der Reader zustandslos und trotzdem konsistent.'
    include
      mcp.xmlEngine,
      mcp.l1Cache,
      sharedCache,
      messageBroker,
      ingestionSystem.indexWriter
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
    description 'Alle Reader-Container (Transport, MCP-Registry, XML-Engine, LOD-Gateway) senden Spans & Metriken an den OTel-Layer; der Writer exportiert direkt. Vor dem OTLP-Export an die externen Dashboards maskiert der PII-Scrubber Mandanten-Daten (Compliance-Gate, ADR-001).'
    include
      *,
      mcp.transport,
      mcp.mcpRegistry,
      mcp.xmlEngine,
      mcp.lodGateway,
      observabilityStack
  }
}
```


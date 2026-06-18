# ADR-005: Service-to-Service-Authentifizierung (mTLS / Zero-Trust intern)

- **Status:** Accepted — interne Redis-Kante implementiert (v7.0)
- **Datum:** 2026-06-01
- **Kontext-Artefakt:** `likec4/` (v6.1) — interne Kanten Reader/Writer → `sharedCache`, `sharedGraphStore`, `semanticService`
- **Betrifft:** `mcp-fedlex` (Reader & Writer), Schnittstelle zu `semantic-fedlex`

## Kontext

ADR-001 sichert die Vertraulichkeit der Daten (PII-Scrubbing, Tenant-Isolation), und der
`authRbac`-Pfad authentifiziert den **Agenten** gegen den Server. Damit ist die Eingangstür
geschützt.

Ungeschützt bleibt das **interne** Netz. Die Kanten Reader → Redis, Reader → Oxigraph,
Reader → semantic-fedlex sowie Writer → Redis und Writer → Oxigraph tragen heute keine
sichtbare gegenseitige Authentifizierung. In einem Mandantenkontext unter anwaltlichem
Berufsgeheimnis (Art. 321 StGB, revDSG) ist ein offenes internes Netz die plausibelste reale
Angriffsfläche, denn dort fliessen entschlüsselte Korpus- und Scratchpad-Daten zwischen den
Diensten.

Ein kompromittierter Pod, ein fehlkonfigurierter NetworkPolicy oder ein Nachbar-Workload im
selben Cluster könnte sonst ungehindert auf Redis (inklusive Tenant-Scratchpad) oder den
Graph-Store zugreifen. Die Tenant-Isolation aus ADR-001 wirkt nur, wenn der Zugriffsweg
selbst authentifiziert ist.

## Entscheidung

Jede dienstübergreifende Verbindung wird **gegenseitig authentifiziert (mTLS)** und durch
eine **Default-Deny-Netzwerk-Policy** ergänzt. Intern gilt Zero-Trust, kein Dienst vertraut
einem anderen allein aufgrund der Netzwerklage.

### Akzeptanzkriterien (Betrieb & Code)
- [x] **mTLS auf der internen Kante.** Reader ↔ Quota-Redis laeuft ueber gegenseitig
      verifizierte Zertifikate (anwendungsseitiges mTLS, ADR-005-Alternative). Im
      Direct-Fetch-Stand (v7.0) ist dies die einzige interne Cluster-Kante des Readers;
      Oxigraph (eingebetteter Korpus) und der Writer-Pfad sind mit dem CQRS-Rueckbau
      entfallen, semantic-fedlex ist im aktuellen Stand nicht verdrahtet.
      (`fedlex-store::RedisTlsConfig` + `RedisQuotaBackend::connect_with_tls`,
      `mcp-reader::main::build_quota_backend`; Manifeste `redis.yaml`/`reader.yaml`)
- [x] **Default-Deny-NetworkPolicy.** Pods akzeptieren nur explizit erlaubte Verbindungen;
      jede nicht deklarierte Kante ist blockiert (`networkpolicy.yaml`, default-deny-all +
      Allowlist DNS/Ingress/Redis/HTTPS).
- [x] **Kurzlebige Identitäten — bewusst manuell.** Dieser Cluster hat kein Mesh und kein
      cert-manager (Edge-TLS via Cloudflare). Die Zertifikate kommen als SealedSecret und
      werden per `gen-redis-mtls.sh` erzeugt/rotiert (dokumentierter Schritt). Eine
      automatische Rotation (SPIFFE/SPIRE) ist bewusst zurueckgestellt — der Preis fuer den
      Verzicht auf ein Mesh.
- [x] **Redis-AUTH zusätzlich.** Redis verlangt zusätzlich zur mTLS-Schicht ein Passwort
      (`requirepass`, Defense-in-Depth). Der Klartext-Port ist abgeschaltet (`--port 0`).
- [n/a] **semantic-fedlex-Grenze.** Im Direct-Fetch-Stand (v7.0) nicht verdrahtet. Greift
      wieder, sobald der Semantic-Pfad zurueckkehrt.
- [x] **Tests.** `fedlex-store::redis_tls`-Unittests (Klartext-Schema abgelehnt, leeres
      Material abgelehnt, Schluessel im Debug redigiert) plus der gegatete
      `redis_isolation`-Integrationstest (Docker). Negativ-Pfad im Code: vorhandenes
      Zertifikatsmaterial bei Klartext-URL laesst den Start hart scheitern.

## Begründung

- **Tenant-Isolation braucht einen authentifizierten Pfad.** ADR-001 schützt die Daten im
  Store, ADR-005 schützt den Weg dorthin. Ohne mTLS bleibt die Isolation umgehbar.
- **Zero-Trust statt Perimeter.** Im geteilten Cluster ist die Netzwerklage kein
  Vertrauensanker. Identität gehört an jede Kante.

## Alternativen

- **Perimeter-Sicherheit (nur Cluster-Firewall).** Verworfen. Schützt nicht vor lateraler
  Bewegung innerhalb des Clusters.
- **Nur NetworkPolicy ohne mTLS.** Unzureichend. Begrenzt die Topologie, aber
  authentifiziert die Gegenstelle nicht und schützt nicht vor Spoofing.
- **Anwendungsseitiges mTLS ohne Mesh.** Tragfähig, aber verlagert Zertifikatsrotation in
  jeden Dienst. **Gewählt**, weil dieser Cluster bewusst ohne Mesh und ohne cert-manager
  läuft (Edge-TLS via Cloudflare). Die Rotation ist ein dokumentierter, manueller Schritt
  (`gen-redis-mtls.sh`).

## Konsequenzen

- **Positiv.** Laterale Bewegung wird unterbunden, die Tenant-Isolation aus ADR-001 wird
  durchsetzbar.
- **Negativ.** Betriebliche Komplexität (eigene CA, manuelle Rotation) und ein kleiner
  Latenz-Overhead pro Verbindung. Für ein berufsgeheimnis-pflichtiges System ist das
  angemessen.
- **Modell.** Querschnitt-Invariante, bewusst **nicht** als eigener Diagramm-Knoten
  modelliert (analog ADR-001 Tenant-Isolation). Sie gilt für alle bestehenden `data`- und
  `http`-Kanten zwischen den Diensten.

---

## Status der Umsetzung
Die einzige interne Cluster-Kante des Readers im Direct-Fetch-Stand (v7.0), Reader ↔
Quota-Redis, ist gegenseitig authentifiziert umgesetzt: anwendungsseitiges mTLS
(`fedlex-store`/`mcp-reader`), Redis nur auf dem TLS-Port mit Client-Zwang und
zusaetzlichem Passwort, Material als SealedSecret via `gen-redis-mtls.sh`,
Default-Deny-NetworkPolicy. Kehren Writer- oder Semantic-Pfad zurueck, gelten die
entsprechenden Kriterien erneut.

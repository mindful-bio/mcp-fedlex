# ADR-005: Service-to-Service-Authentifizierung (mTLS / Zero-Trust intern)

- **Status:** Accepted (Plan / v6.1)
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
- [ ] **mTLS auf allen internen Kanten.** Reader/Writer ↔ Redis, ↔ Oxigraph,
      ↔ semantic-fedlex laufen über gegenseitig verifizierte Zertifikate (Service-Mesh wie
      Linkerd/Istio oder anwendungsseitiges mTLS).
- [ ] **Default-Deny-NetworkPolicy.** Pods akzeptieren nur explizit erlaubte Verbindungen;
      jede nicht deklarierte Kante ist blockiert (Allowlist, analog ADR-001 Prinzip).
- [ ] **Kurzlebige Identitäten.** Dienst-Zertifikate werden automatisch rotiert (z.B.
      SPIFFE/SPIRE oder Mesh-eigene CA), keine langlebigen statischen Secrets im Pod.
- [ ] **Redis-AUTH zusätzlich.** Redis verlangt zusätzlich zur mTLS-Schicht ein
      dienstspezifisches Credential (Defense-in-Depth, ergänzt die Tenant-ACL aus ADR-001).
- [ ] **semantic-fedlex-Grenze.** Der Aufruf `embeddingOutbox -> semanticService` und
      `semanticClient -> semanticService` ist beidseitig authentifiziert; der GPU-Dienst
      akzeptiert nur bekannte Aufrufer.
- [ ] **Tests.** Negativtest, der belegt, dass ein nicht-authentifizierter In-Cluster-Client
      Redis/Oxigraph/semantic-fedlex nicht erreicht.

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
  jeden Dienst. Mesh bevorzugt, sofern die Plattform es trägt.

## Konsequenzen

- **Positiv.** Laterale Bewegung wird unterbunden, die Tenant-Isolation aus ADR-001 wird
  durchsetzbar, der `semantic-fedlex`-Aufruf ist beidseitig vertrauenswürdig.
- **Negativ.** Betriebliche Komplexität (CA, Rotation, Mesh) und ein kleiner Latenz-Overhead
  pro Verbindung. Für ein berufsgeheimnis-pflichtiges System ist das angemessen.
- **Modell.** Querschnitt-Invariante, bewusst **nicht** als eigener Diagramm-Knoten
  modelliert (analog ADR-001 Tenant-Isolation). Sie gilt für alle bestehenden `data`- und
  `http`-Kanten zwischen den Diensten.

---

## Status der Umsetzung
Architektur-Plan (LikeC4 v6.1) festgehalten. Implementierung offen — diese ADR ist die
verbindliche Akzeptanzkriterien-Liste für das spätere Coding.

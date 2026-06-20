# 70 — Konfigurationsreferenz (Umgebungsvariablen)

> **Was dieses Dokument ist.** Die vollständige Referenz aller Umgebungsvariablen, die der
> `mcp-reader` zur Laufzeit liest. Quelle: `crates/mcp-reader/src/main.rs`. Es gibt **keine**
> Konfigurationsdatei — die gesamte Konfiguration läuft über die Umgebung (12-Factor).

---

## 1. Netzwerk

| Variable | Pflicht | Default | Beschreibung |
| --- | --- | --- | --- |
| `BIND_ADDR` | nein | `0.0.0.0:8080` | Socket, auf dem der Reader lauscht (`HOST:PORT`). |
| `REDIS_URL` | nein | `redis://127.0.0.1:6379` | Quota-Backend (verteiltes Token-Bucket). Bei aktiviertem mTLS **muss** das Schema `rediss://` sein. |

## 1b. Protokoll-Negotiation (ADR-008 · Quelle: `src/protocol.rs`)

Der `initialize`-Handshake handelt die MCP-Protokollversion aus. Nennt der Client eine
**unterstützte** Version, wird diese ausgehandelt; nennt er **keine** (heutiger ansV-Fall), gilt die
Default-Version; nennt er eine **unbekannte/zu neue**, antwortet der Reader spec-konform mit seiner
höchsten unterstützten (kein harter Fehler). Heute wird nur `2024-11-05` unterstützt — der Sprung
auf eine neuere Revision erfolgt kontrolliert nach dem Migrations-Runbook
(`docs/55_MIGRATION_mcp_protocol_upgrade.md`).

| Variable | Pflicht | Default | Beschreibung |
| --- | --- | --- | --- |
| `MCP_PROTOCOL_DEFAULT` | nein | `2024-11-05` | Ausgehandelte Default-Version für Clients **ohne** `protocolVersion`. Wird **nur** akzeptiert, wenn der Wert in `SUPPORTED_PROTOCOL_VERSIONS` steht; sonst fail-safe auf die Kompilier-Default. Erlaubt den späteren Versionssprung als **Config-Flip ohne Redeploy** (Runbook Phase 6.2). |

## 2. Authentifizierung

Der Reader ist **fail-closed**: Ohne gültige Auth-Konfiguration ist **kein** Credential gültig und
jeder Aufruf endet mit `-32001 missing/invalid credential`. Die Auswahl erfolgt in dieser
**Reihenfolge** (erste passende gewinnt):

1. `MCP_JWT_JWKS_URL` → JWKS-Modus (rotierende Schlüssel vom IdP)
2. `MCP_JWT_HS256_SECRET` → HS256 mit statischem Secret
3. `MCP_JWT_RS256_PUBKEY_FILE` → RS256 mit PEM-Public-Key
4. `MCP_DEV_TOKEN` → statisches Dev-Token (Rolle `Validator`, Mandant `dev`)

| Variable | Pflicht | Default | Beschreibung |
| --- | --- | --- | --- |
| `MCP_JWT_ISSUER` | bedingt | — | **Pflicht in jedem JWT-Modus.** Erwarteter `iss`-Claim. |
| `MCP_JWT_AUDIENCE` | nein | — | Optionaler `aud`-Claim. Wenn gesetzt, wird er geprüft. |
| `MCP_JWT_JWKS_URL` | nein | — | JWKS-Endpunkt. Aktiviert den JWKS-Modus. |
| `MCP_JWT_JWKS_REFRESH_SECS` | nein | `300` | Abrufintervall des JWKS in Sekunden. |
| `MCP_JWT_HS256_SECRET` | nein | — | Symmetrisches Secret (HS256). |
| `MCP_JWT_RS256_PUBKEY_FILE` | nein | — | Pfad zu einer PEM-Datei mit RSA-Public-Key (RS256). |
| `MCP_DEV_TOKEN` | nein | — | Statisches Bearer-Token für die lokale Entwicklung. **Nicht in Produktion.** |

> Rollenmodell und Claims-Schema (`role`/`tenant`/`session`): siehe `docs/90_AUTH_AND_ROLES.md`.

## 3. Redis-mTLS (optional, Produktion · ADR-005)

Diese drei Variablen müssen **gemeinsam** gesetzt sein (alle drei oder keine). Sind sie gesetzt,
verbindet sich der Reader gegenseitig authentifiziert über `rediss://`; `REDIS_URL` **muss** dann
mit `rediss://` beginnen, sonst bricht der Start bewusst ab (kein stillschweigender Klartext).

| Variable | Pflicht | Default | Beschreibung |
| --- | --- | --- | --- |
| `MCP_REDIS_TLS_CA_FILE` | bedingt | — | CA-Zertifikat (PEM), das das Redis-Server-Zert beglaubigt. |
| `MCP_REDIS_TLS_CERT_FILE` | bedingt | — | Client-Zertifikat (PEM) des Readers. |
| `MCP_REDIS_TLS_KEY_FILE` | bedingt | — | Privater Schlüssel (PEM) zum Client-Zert. |

Ohne dieses Material bleibt die Redis-Verbindung Klartext — im Cluster abgesichert durch die
Default-Deny-NetworkPolicy, lokal durch das interne compose-Netz.

## 4. Health-Endpunkte (keine Konfiguration, zur Referenz)

| Pfad | Zweck |
| --- | --- |
| `GET /livez` | Liveness — anschlagsfrei, prüft keinen Upstream. |
| `GET /readyz` | Readiness — prüft Quota-Redis und den Fedlex-SPARQL-Endpunkt. |
| `GET /startupz` | Startup — wird grün, sobald der (derzeit leere) Warmup durch ist. |

## 5. Minimalbeispiele

**Lokal (Dev-Token, Klartext-Redis):**
```bash
BIND_ADDR=0.0.0.0:8080
REDIS_URL=redis://redis:6379
MCP_DEV_TOKEN=dev-secret-change-me
```

**Produktion (JWKS + Redis-mTLS):**
```bash
BIND_ADDR=0.0.0.0:8080
REDIS_URL=rediss://mcp-reader-redis:6379
MCP_JWT_ISSUER=https://idp.example.com/
MCP_JWT_AUDIENCE=mcp-fedlex
MCP_JWT_JWKS_URL=https://idp.example.com/.well-known/jwks.json
MCP_REDIS_TLS_CA_FILE=/etc/redis-tls/ca.crt
MCP_REDIS_TLS_CERT_FILE=/etc/redis-tls/client.crt
MCP_REDIS_TLS_KEY_FILE=/etc/redis-tls/client.key
```

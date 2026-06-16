//! mcp-reader - zustandsloser MCP-Reader (Direct Fetch).
//!
//! Binary-Entrypoint. Liest die Betriebsparameter aus der Umgebung, baut den
//! Dienst aus den Bausteinen der Bibliothek und serviert die zusammengesetzte
//! App ([`mcp_reader::app`]) hinter einem echten Socket. Die Routing- und
//! Kompositionslogik ist in der Lib per `cargo test` bewiesen. Diese Datei ist
//! nur die dünne Verkabelung aus Umgebung zu Netzwerk.
//!
//! Die Registry trägt die produktiven Navigations-Tools über der
//! fedlex-bridge (Direct Fetch gegen den Fedlex-SPARQL-Endpunkt und den
//! AKN-Filestore des Bundes). Credentials kommen wahlweise von einem IdP
//! (JWT, HS256/RS256 mit statischem Schlüsselmaterial) oder im Dev-Betrieb
//! aus MCP_DEV_TOKEN. Ohne beides bleibt der Server fail-closed.

use std::net::SocketAddr;
use std::sync::Arc;

use fedlex_bridge::{AknFetcher, HttpSparqlClient, HttpXmlSource};
use mcp_reader::app::{app, serve};
use mcp_reader::auth::{AuthResolver, JwksAuthResolver, JwtAuthResolver, StaticAuthResolver};
use mcp_reader::discovery::register_discovery_tools;
use mcp_reader::health::HealthState;
use mcp_reader::metadata::register_metadata_tools;

use mcp_reader::probes::{QuotaBackendProbe, SparqlProbe};
use mcp_reader::quota::{QuotaPolicy, RateLimiter, RedisQuotaBackend};
use mcp_reader::registry::Registry;
use mcp_reader::temporal::TemporalResolver;
use mcp_reader::tools::register_navigation_tools;

use mcp_reader::transport::McpService;
use tokio::net::TcpListener;

/// Kapazität des Manifestations-Caches (geparste Erlasse pro Pod).
const FETCHER_CACHE_CAPACITY: u64 = 64;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let addr: SocketAddr = bind_addr.parse()?;

    let backend = RedisQuotaBackend::connect(&redis_url)?;
    let limiter = RateLimiter::with_policy(backend.clone(), QuotaPolicy::default());

    // Credential-Herkunft zur Laufzeit. JWT-Konfiguration gewinnt vor dem
    // Dev-Token. Ohne beides bleibt der Resolver leer (fail-closed, kein
    // einziges Credential gültig).
    let auth: Box<dyn AuthResolver + Send + Sync> = build_auth_resolver()?;

    // Direct Fetch. Ein Fetcher (und damit ein Manifestations-Cache) für alle
    // Navigations-Tools dieses Pods.
    let fetcher = Arc::new(AknFetcher::new(
        HttpSparqlClient::fedlex(),
        HttpXmlSource::new(),
        FETCHER_CACHE_CAPACITY,
    ));
    let mut registry = Registry::new();
    register_navigation_tools(&mut registry, fetcher);

    // Discovery-Tools (ADR-006). Eigener SPARQL-Client für die Live-Auflösung
    // gegen Fedlex (Suche/SR-Auflösung/Themen). Sie liefern Kandidaten-ELIs mit
    // Hinweis-Provenance und sind nur Navigator/Validator sichtbar.
    register_discovery_tools(&mut registry, Arc::new(HttpSparqlClient::fedlex()));

    // JOLux-Metadaten-Tools (ADR-007, Tranche A: Temporal). Eigener SPARQL-
    // Client für die Live-Auflösung gegen Fedlex. Sie belegen Eigenschaften
    // eines bekannten Erlasses (Norm-Provenance) und sind, wie Discovery, nur
    // Navigator/Validator sichtbar und im Quota gleich gewichtet.
    register_metadata_tools(&mut registry, Arc::new(HttpSparqlClient::fedlex()));

    let today = time::OffsetDateTime::now_utc().date();

    let temporal = TemporalResolver::new(today);
    let service = Arc::new(McpService::new(registry, auth, limiter, temporal));

    let health = Arc::new(
        HealthState::new()
            .with_probe(Arc::new(QuotaBackendProbe::new(backend)))
            .with_probe(Arc::new(SparqlProbe::new(HttpSparqlClient::fedlex()))),
    );

    let listener = TcpListener::bind(addr).await?;
    println!("mcp-reader lauscht auf {addr}");

    // Der Aufwärmlauf ist hier noch leer. Sobald er füllt, läuft er vor dieser
    // Marke. Erst danach meldet startupz die Bereitschaft.
    health.mark_started();

    serve(listener, app(service, Arc::clone(&health))).await?;
    Ok(())
}

/// Wählt den Auth-Resolver anhand der Umgebung.
///
/// Reihenfolge. Erst MCP_JWT_JWKS_URL (rotierende Schlüssel vom IdP), dann
/// MCP_JWT_HS256_SECRET, dann MCP_JWT_RS256_PUBKEY_FILE (PEM-Pfad), zuletzt
/// MCP_DEV_TOKEN. Im JWT-Modus ist MCP_JWT_ISSUER Pflicht und
/// MCP_JWT_AUDIENCE optional.
fn build_auth_resolver() -> Result<Box<dyn AuthResolver + Send + Sync>, Box<dyn std::error::Error>>
{
    let issuer = std::env::var("MCP_JWT_ISSUER").ok();
    let audience = std::env::var("MCP_JWT_AUDIENCE").ok();

    if let Ok(url) = std::env::var("MCP_JWT_JWKS_URL") {
        let issuer = issuer.ok_or("MCP_JWT_ISSUER ist im JWT-Modus Pflicht")?;
        let refresh_secs: u64 = std::env::var("MCP_JWT_JWKS_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let resolver = Arc::new(JwksAuthResolver::new(issuer.clone(), audience));
        spawn_jwks_refresher(Arc::clone(&resolver), url.clone(), refresh_secs);
        println!("JWT-Auth aktiv (JWKS {url}, Issuer {issuer}, Refresh {refresh_secs}s)");
        return Ok(Box::new(resolver));
    }

    if let Ok(secret) = std::env::var("MCP_JWT_HS256_SECRET") {
        let issuer = issuer.ok_or("MCP_JWT_ISSUER ist im JWT-Modus Pflicht")?;
        println!("JWT-Auth aktiv (HS256, Issuer {issuer})");
        return Ok(Box::new(JwtAuthResolver::hs256(
            secret.as_bytes(),
            &issuer,
            audience.as_deref(),
        )));
    }

    if let Ok(path) = std::env::var("MCP_JWT_RS256_PUBKEY_FILE") {
        let issuer = issuer.ok_or("MCP_JWT_ISSUER ist im JWT-Modus Pflicht")?;
        let pem = std::fs::read(&path)?;
        println!("JWT-Auth aktiv (RS256, Issuer {issuer}, Key {path})");
        return Ok(Box::new(
            JwtAuthResolver::rs256_pem(&pem, &issuer, audience.as_deref())
                .map_err(|_| format!("ungueltiger RSA-Public-Key in {path}"))?,
        ));
    }

    let mut auth = StaticAuthResolver::new();
    if let Ok(token) = std::env::var("MCP_DEV_TOKEN") {
        auth = auth.with_credential(
            token,
            mcp_reader::auth::ClaimRecord {
                tenant: "dev".into(),
                session: "dev".into(),
                role: mcp_reader::auth::Role::Validator,
            },
        );
        println!("MCP_DEV_TOKEN aktiv (Rolle Validator, Mandant dev)");
    } else {
        println!("Keine Auth-Konfiguration. Server bleibt fail-closed (kein Credential gueltig).");
    }
    Ok(Box::new(auth))
}

/// Periodischer JWKS-Abruf. Erster Lauf sofort, danach im Intervall.
///
/// Fehler beim Abruf lassen den bisherigen Schlüsselsatz unangetastet
/// (Verfügbarkeit vor Frische). Bis zum ersten Erfolg ist der Satz leer
/// und der Resolver fail-closed.
fn spawn_jwks_refresher(resolver: Arc<JwksAuthResolver>, url: String, refresh_secs: u64) {
    tokio::spawn(async move {
        let http = reqwest::Client::new();
        loop {
            match http.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => match resp.text().await {
                    Ok(body) => match resolver.install_jwks(&body) {
                        Ok(n) => println!("JWKS aktualisiert ({n} Schluessel)"),
                        Err(_) => eprintln!("JWKS nicht parsebar, alter Satz bleibt aktiv"),
                    },
                    Err(e) => eprintln!("JWKS-Abruf fehlgeschlagen: {e}"),
                },
                Ok(resp) => eprintln!("JWKS-Endpunkt antwortete {}", resp.status()),
                Err(e) => eprintln!("JWKS-Abruf fehlgeschlagen: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(refresh_secs)).await;
        }
    });
}

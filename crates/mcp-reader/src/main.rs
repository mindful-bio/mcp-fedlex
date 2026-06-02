//! mcp-reader - zustandsloser MCP-Reader (CQRS-Leseseite).
//!
//! Binary-Entrypoint. Liest die Betriebsparameter aus der Umgebung, baut den
//! Dienst aus den Bausteinen der Bibliothek und serviert die zusammengesetzte
//! App ([`mcp_reader::app`]) hinter einem echten Socket. Die Routing- und
//! Kompositionslogik ist in der Lib per `cargo test` bewiesen. Diese Datei ist
//! nur die dünne Verkabelung aus Umgebung zu Netzwerk.
//!
//! Bewusst noch offen und in eigenen Schritten verdrahtet. Die Tool-Registrierung
//! der Registry, die Herkunft der Credentials und die Readiness-Proben gegen die
//! Backends. Bis dahin ist dies ein lauffähiges, deploybares Skelett, dessen
//! Health-Endpunkte real antworten.

use std::net::SocketAddr;
use std::sync::Arc;

use mcp_reader::app::{app, serve};
use mcp_reader::auth::StaticAuthResolver;
use mcp_reader::health::HealthState;
use mcp_reader::quota::{QuotaPolicy, RateLimiter, RedisQuotaBackend};
use mcp_reader::registry::Registry;
use mcp_reader::temporal::TemporalResolver;
use mcp_reader::transport::McpService;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let addr: SocketAddr = bind_addr.parse()?;

    let backend = RedisQuotaBackend::connect(&redis_url)?;
    let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());
    let auth = StaticAuthResolver::new();
    let registry = Registry::new();
    let today = time::OffsetDateTime::now_utc().date();
    let temporal = TemporalResolver::new(today);
    let service = Arc::new(McpService::new(registry, auth, limiter, temporal));

    let health = Arc::new(HealthState::new());

    let listener = TcpListener::bind(addr).await?;
    println!("mcp-reader lauscht auf {addr}");

    // Der Aufwärmlauf ist hier noch leer. Sobald er füllt, läuft er vor dieser
    // Marke. Erst danach meldet startupz die Bereitschaft.
    health.mark_started();

    serve(listener, app(service, Arc::clone(&health))).await?;
    Ok(())
}

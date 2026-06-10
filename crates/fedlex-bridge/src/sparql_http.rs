//! Produktions-Implementierung des [`SparqlClient`]-Traits über HTTP.
//!
//! Die jolux-Primitive sind transportfrei — dieser Client liefert den
//! Live-Transport gegen `fedlex.data.admin.ch` (oder einen Spiegel).

use async_trait::async_trait;
use fedlex_jolux::{JoluxError, SparqlClient, SparqlResults};

/// Standard-Endpoint des öffentlichen Fedlex-Triplestores.
pub const FEDLEX_ENDPOINT: &str = "https://fedlex.data.admin.ch/sparqlendpoint";

/// HTTP-SPARQL-Client (POST `application/x-www-form-urlencoded`,
/// Accept `application/sparql-results+json`).
///
/// Achtung Falltrap (Live-Befund 2026-06-10, JOLux-Lexikon). Die Fedlex-WAF
/// blockt bestimmte Query-Formen (`SELECT DISTINCT` plus
/// `citationFromLegalResource` plus URL-Literal ergibt HTTP 400). Die
/// jolux-Primitive sind entsprechend formuliert. Eigene Queries durch diesen
/// Client müssen das ebenfalls beachten.
#[derive(Debug, Clone)]
pub struct HttpSparqlClient {
    http: reqwest::Client,
    endpoint: String,
}

impl HttpSparqlClient {
    /// Client gegen den gegebenen Endpoint.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.into(),
        }
    }

    /// Client gegen den öffentlichen Fedlex-Endpoint.
    pub fn fedlex() -> Self {
        Self::new(FEDLEX_ENDPOINT)
    }
}

#[async_trait]
impl SparqlClient for HttpSparqlClient {
    async fn query(&self, sparql: &str) -> Result<SparqlResults, JoluxError> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header("Accept", "application/sparql-results+json")
            .form(&[("query", sparql)])
            .send()
            .await
            .map_err(|e| JoluxError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(JoluxError::Transport(format!(
                "HTTP {} vom Endpoint",
                resp.status()
            )));
        }
        let body = resp
            .text()
            .await
            .map_err(|e| JoluxError::Transport(e.to_string()))?;
        SparqlResults::from_json(&body)
    }
}

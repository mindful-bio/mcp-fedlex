//! Abstraktion über den XML-Download (Manifestations-URL → Rohtext).
//!
//! Analog zum [`SparqlClient`]-Trait der jolux-Crate hält dieses Trait die
//! Fetcher-Logik transportfrei und deterministisch testbar.
//!
//! [`SparqlClient`]: fedlex_jolux::SparqlClient

use crate::error::BridgeError;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Quelle für AKN-XML-Manifestationen.
#[async_trait]
pub trait XmlSource: Send + Sync {
    /// Lädt das XML hinter der gegebenen Manifestations-URL.
    async fn fetch(&self, url: &str) -> Result<String, BridgeError>;
}

/// Produktions-Quelle über HTTP (`fedlex.data.admin.ch/filestore/...`).
#[derive(Debug, Clone, Default)]
pub struct HttpXmlSource {
    http: reqwest::Client,
}

impl HttpXmlSource {
    /// Neue HTTP-Quelle mit eigenem Client.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl XmlSource for HttpXmlSource {
    async fn fetch(&self, url: &str) -> Result<String, BridgeError> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| BridgeError::Download(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(BridgeError::Download(format!(
                "HTTP {} für {url}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| BridgeError::Download(e.to_string()))
    }
}

/// Deterministische Mock-Quelle für Tests. Liefert für jede URL dasselbe
/// vorbereitete XML und zählt die Abrufe (für Cache-Assertions).
#[derive(Debug, Clone)]
pub struct MockXmlSource {
    xml: String,
    fetches: Arc<AtomicUsize>,
}

impl MockXmlSource {
    /// Mock-Quelle mit dem gegebenen XML-Inhalt.
    pub fn new(xml: impl Into<String>) -> Self {
        Self {
            xml: xml.into(),
            fetches: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Wie oft `fetch` aufgerufen wurde.
    pub fn fetch_count(&self) -> usize {
        self.fetches.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl XmlSource for MockXmlSource {
    async fn fetch(&self, _url: &str) -> Result<String, BridgeError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        Ok(self.xml.clone())
    }
}

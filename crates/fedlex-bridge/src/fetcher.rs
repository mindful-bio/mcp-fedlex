//! Die eigentliche Brücke. Komponiert JLX-TMP-02 (Stichtags-Auflösung der
//! Konsolidierung inkl. XML-URL) mit AKN-DOC-01 (Parse) und cached geparste
//! Dokumente pro Manifestations-URL.
//!
//! Die URL ist der ideale Cache-Schlüssel, weil sie pro Konsolidierung,
//! Sprache und Format eindeutig und unveränderlich ist (Manifestationen im
//! Filestore sind immutable). Verschiedene Stichtage, die auf dieselbe
//! Fassung auflösen, treffen damit denselben Cache-Eintrag.

use crate::error::BridgeError;
use crate::xml_source::XmlSource;
use fedlex_akn::AknDocument;
use fedlex_core::{Eli, Response, ValidAsOf};
use fedlex_jolux::{resolve_consolidation_at, Language, SparqlClient};
use moka::future::Cache;
use std::sync::Arc;

/// Beschafft AKN-Dokumente. Generisch über SPARQL-Transport und XML-Quelle —
/// produktiv [`HttpSparqlClient`] + [`HttpXmlSource`], im Test Mocks.
///
/// [`HttpSparqlClient`]: crate::HttpSparqlClient
/// [`HttpXmlSource`]: crate::HttpXmlSource
pub struct AknFetcher<C, S> {
    sparql: C,
    source: S,
    cache: Cache<String, Arc<AknDocument>>,
}

impl<C: SparqlClient, S: XmlSource> AknFetcher<C, S> {
    /// Fetcher mit Cache für maximal `capacity` geparste Dokumente.
    ///
    /// Richtwert: ein konsolidierter Erlass ist als geparster Baum grob
    /// 1–10 MB. 64 Einträge decken eine typische Agenten-Session ab.
    pub fn new(sparql: C, source: S, capacity: u64) -> Self {
        Self {
            sparql,
            source,
            cache: Cache::new(capacity),
        }
    }

    /// AKN-DOC-01 (produktiv): liefert das geparste AKN-Dokument eines
    /// Erlasses zum Stichtag in der gewünschten Sprache.
    ///
    /// Ablauf: JLX-TMP-02 löst Konsolidierung + XML-URL auf, dann Download +
    /// Parse (gecached). Liefert [`JoluxError::NotFound`], wenn zum Stichtag
    /// keine XML-Manifestation existiert — XML gibt es im Filestore erst ab
    /// ~2021 (Live-Befund 2026-06-10, ältere Fassungen nur doc/pdf).
    ///
    /// Die Provenance stammt aus der JOLux-Auflösung (Work-ELI + Stichtag +
    /// Systemzeit, ADR-004).
    ///
    /// [`JoluxError::NotFound`]: fedlex_jolux::JoluxError::NotFound
    pub async fn fetch_akn_document(
        &self,
        eli: &Eli,
        as_of: ValidAsOf,
        lang: Language,
    ) -> Result<Response<Arc<AknDocument>>, BridgeError> {
        let cons = resolve_consolidation_at(&self.sparql, eli, as_of, lang).await?;
        let prov = cons.provenance().clone();
        let url = cons.data().xml_url.clone();

        if let Some(doc) = self.cache.get(&url).await {
            return Ok(Response::new(doc, prov));
        }

        // Bewusst kein Single-Flight: bei parallelem Miss wird schlimmstenfalls
        // doppelt geparst, das Ergebnis ist identisch (URL ist immutable).
        let xml = self.source.fetch(&url).await?;
        let doc = Arc::new(AknDocument::parse(&xml)?);
        self.cache.insert(url, Arc::clone(&doc)).await;
        Ok(Response::new(doc, prov))
    }
}

//! fedlex-bridge — der produktive Pfad von JOLux (Metadaten) zu AKN (Volltext).
//!
//! Die Domänen-Crates sind bewusst transportfrei. Diese Crate liefert die
//! fehlenden Transport-Bausteine und komponiert sie zur Funktion
//! `fetch_akn_document` (AKN-DOC-01 produktiv):
//!
//! 1. [`HttpSparqlClient`] — erste Produktions-Implementierung des
//!    [`SparqlClient`]-Traits aus fedlex-jolux.
//! 2. [`XmlSource`]/[`HttpXmlSource`] — Download der AKN-XML-Manifestation.
//! 3. [`AknFetcher`] — Komposition JLX-TMP-02 → Download → Parse, mit Cache
//!    pro Manifestations-URL.
//!
//! Die Konformanz dieser Komposition gegen den Live-Endpoint prüft
//! `tests/bridge_conformance.rs` (`--ignored`).
//!
//! [`SparqlClient`]: fedlex_jolux::SparqlClient

#![forbid(unsafe_code)]

pub mod error;
pub mod fetcher;
pub mod sparql_http;
pub mod xml_source;

pub use error::BridgeError;
pub use fetcher::AknFetcher;
pub use sparql_http::{FEDLEX_ENDPOINT, HttpSparqlClient};
pub use xml_source::{HttpXmlSource, MockXmlSource, XmlSource};

#[cfg(test)]
mod tests {
    use super::*;
    use fedlex_core::{Eli, ValidAsOf};
    use fedlex_jolux::{Language, MockSparqlClient};
    use time::macros::date;

    /// Canned-Ergebnis für JLX-TMP-02 (Variablen cons/date/url).
    const CONS_JSON: &str = r#"{
      "head": { "vars": ["cons", "date", "url"] },
      "results": { "bindings": [ {
        "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20260401" },
        "date": { "type": "literal", "value": "2026-04-01" },
        "url": { "type": "uri", "value": "https://fedlex.data.admin.ch/filestore/x/de/xml" }
      } ] }
    }"#;

    /// Minimales, aber strukturell echtes AKN-Dokument (Akt mit einem Artikel).
    const MINI_ACT: &str = r##"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
      <act>
        <meta><identification source="#me">
          <FRBRWork>
            <FRBRuri value="https://fedlex.data.admin.ch/eli/cc/2017/762/20260401"/>
            <FRBRname xml:lang="de" value="Energiegesetz"/>
          </FRBRWork>
          <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
        </identification></meta>
        <body>
          <article eId="art_1">
            <num>Art. 1</num>
            <paragraph eId="art_1/para_1"><content><p>Zweck.</p></content></paragraph>
          </article>
        </body>
      </act>
    </akomaNtoso>"##;

    fn fetcher() -> (AknFetcher<MockSparqlClient, MockXmlSource>, MockXmlSource) {
        let sparql = MockSparqlClient::from_json(CONS_JSON);
        let source = MockXmlSource::new(MINI_ACT);
        (AknFetcher::new(sparql, source.clone(), 8), source)
    }

    #[tokio::test]
    async fn fetches_resolves_and_parses() {
        let (f, _) = fetcher();
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = f
            .fetch_akn_document(&eli, ValidAsOf::new(date!(2026 - 06 - 01)), Language::De)
            .await
            .expect("fetch");
        assert!(!resp.data().lookup_eid("art_1").is_empty());
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn second_call_hits_cache() {
        let (f, source) = fetcher();
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let as_of = ValidAsOf::new(date!(2026 - 06 - 01));
        f.fetch_akn_document(&eli, as_of, Language::De)
            .await
            .unwrap();
        f.fetch_akn_document(&eli, as_of, Language::De)
            .await
            .unwrap();
        assert_eq!(
            source.fetch_count(),
            1,
            "zweiter Abruf muss aus dem Cache kommen"
        );
    }

    #[tokio::test]
    async fn missing_consolidation_propagates_not_found() {
        let sparql = MockSparqlClient::from_json(
            r#"{ "head": { "vars": ["cons","date","url"] }, "results": { "bindings": [] } }"#,
        );
        let f = AknFetcher::new(sparql, MockXmlSource::new(""), 8);
        let eli = Eli::new("eli/cc/1907/233").unwrap();
        let err = f
            .fetch_akn_document(&eli, ValidAsOf::new(date!(2000 - 01 - 01)), Language::De)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BridgeError::Jolux(fedlex_jolux::JoluxError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn broken_xml_propagates_akn_error() {
        let sparql = MockSparqlClient::from_json(CONS_JSON);
        let f = AknFetcher::new(sparql, MockXmlSource::new("<kaputt"), 8);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let err = f
            .fetch_akn_document(&eli, ValidAsOf::new(date!(2026 - 06 - 01)), Language::De)
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::Akn(_)));
    }
}

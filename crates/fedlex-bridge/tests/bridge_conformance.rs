//! Konformanz-Suite der Brücke gegen den **Live**-Endpoint.
//!
//! Beweist die Komposition jolux → Download → akn als Ganzes (Review-Befund
//! #2). Alle Tests sind `#[ignore]` und laufen nur auf Verlangen:
//!
//! ```sh
//! cargo test -p fedlex-bridge --test bridge_conformance -- --ignored --test-threads 2
//! ```
//!
//! Referenz-Erlass: Energiegesetz (EnG, SR 730.0, eli/cc/2017/762) —
//! identisch zur AKN-Konformanz-Suite.

use fedlex_bridge::{AknFetcher, HttpSparqlClient, HttpXmlSource};
use fedlex_core::{Eli, ValidAsOf};
use fedlex_jolux::{JoluxError, Language};
use time::macros::date;

const ENG_WORK: &str = "eli/cc/2017/762";

fn fetcher() -> AknFetcher<HttpSparqlClient, HttpXmlSource> {
    AknFetcher::new(HttpSparqlClient::fedlex(), HttpXmlSource::new(), 8)
}

fn stichtag() -> ValidAsOf {
    ValidAsOf::new(date!(2026 - 06 - 01))
}

/// Volle Kette DE: Auflösen, Laden, Parsen — und die akn-Primitive arbeiten
/// auf dem Ergebnis (DOC-02, TXT-01).
#[tokio::test]
#[ignore = "Live-Test gegen fedlex.data.admin.ch"]
async fn full_chain_resolves_and_parses_eng_de() {
    let f = fetcher();
    let eli = Eli::new(ENG_WORK).unwrap();
    let resp = f
        .fetch_akn_document(&eli, stichtag(), Language::De)
        .await
        .expect("EnG muss zum Stichtag als XML auflösbar sein");

    // Provenance trägt das ungedatete Work-ELI (ADR-004 + Review-Fix B2).
    assert_eq!(resp.provenance().eli.as_str(), ENG_WORK);

    let doc = resp.data();
    let meta = fedlex_akn::get_frbr_metadata(doc).expect("FRBR");
    assert_eq!(meta.language.as_deref(), Some("de"));
    assert!(
        meta.title
            .as_deref()
            .unwrap_or_default()
            .contains("Energie"),
        "Titel war {:?}",
        meta.title
    );

    let art1 = fedlex_akn::get_article_text(doc, "art_1", stichtag()).expect("Art. 1");
    assert!(!art1.data().text.is_empty(), "Art. 1 EnG muss Text liefern");
}

/// Volle Kette FR: dieselbe Konsolidierung in zweiter Amtssprache
/// (mildert Review-Befund #4, DE-Monokultur der Tests).
#[tokio::test]
#[ignore = "Live-Test gegen fedlex.data.admin.ch"]
async fn full_chain_works_in_french() {
    let f = fetcher();
    let eli = Eli::new(ENG_WORK).unwrap();
    let resp = f
        .fetch_akn_document(&eli, stichtag(), Language::Fr)
        .await
        .expect("EnG muss auch als FR-XML auflösbar sein");
    let meta = fedlex_akn::get_frbr_metadata(resp.data()).expect("FRBR");
    assert_eq!(meta.language.as_deref(), Some("fr"));
}

/// Stichtag vor Inkrafttreten: die Brücke gibt den jolux-NotFound ehrlich
/// weiter statt stillschweigend die neueste Fassung zu liefern (J3.1/J20.2).
#[tokio::test]
#[ignore = "Live-Test gegen fedlex.data.admin.ch"]
async fn pre_enactment_date_yields_not_found() {
    let f = fetcher();
    let eli = Eli::new(ENG_WORK).unwrap();
    let err = f
        .fetch_akn_document(&eli, ValidAsOf::new(date!(1990 - 01 - 01)), Language::De)
        .await
        .expect_err("1990 gab es kein EnG");
    assert!(matches!(
        err,
        fedlex_bridge::BridgeError::Jolux(JoluxError::NotFound(_))
    ));
}

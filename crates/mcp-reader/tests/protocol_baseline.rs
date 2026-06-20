//! # Protokoll-Baseline (Sicherheitsnetz VOR dem MCP-Upgrade)
//!
//! Phase 1 des Migrations-Runbooks
//! ([`docs/55_MIGRATION_mcp_protocol_upgrade.md`](../../../docs/55_MIGRATION_mcp_protocol_upgrade.md),
//! Schritte 1.1 + 1.2). Dieser Offline-Test **friert den heutigen Konsumenten-
//! Vertrag ein**, bevor irgendeine Verhaltensänderung beginnt. Er wird **rot**,
//! sobald sich der `initialize`-Handshake, die Antwortform von `tools/list`/
//! `tools/call` oder das fail-closed-Auth-/Methoden-Verhalten ändert.
//!
//! Damit ist die zentrale Migrationsregel mechanisch geschützt: Der einzige
//! bekannte Produktions-Konsument (ansV) ruft **kein `initialize`** und greift
//! **direkt auf `/rpc`** mit `tools/list`/`tools/call` zu. Jede Migration muss
//! **additiv** bleiben — bricht sie diesen Vertrag, bricht dieser Test.
//!
//! Verifizierte Ist-Werte (nach Phase 6.3 Pull-Through):
//! - `protocolVersion == "2025-11-25"` (ausgehandelte Default-Revision; ein
//!   explizit `2024-11-05` anfragender Client erhält weiterhin `2024-11-05`)
//! - `serverInfo.name == "mcp-fedlex-reader"`
//! - `capabilities == { "tools": {} }` (Lifecycle ping/initialized aktiv, aber
//!   nicht als Capability angekündigt, da kein Capability-Flag nötig)

//! - `tools/list`-Eintragsform: `{ "name", "inputSchema", "schema" }` — der
//!   MCP-Standardschlüssel `inputSchema` UND der Legacy-Schlüssel `schema`
//!   tragen denselben Wert (additive Migration, ADR-008 §A; Legacy fällt erst
//!   in Runbook-Phase 9, wenn alle Clients umgestellt sind)

//!
//! **Kein `#[ignore]`** — rein offline, läuft in jeder `cargo test`-Runde.
//!
//! ```sh
//! cargo test -p mcp-reader --test protocol_baseline
//! ```

use std::sync::Arc;

use fedlex_bridge::{AknFetcher, MockXmlSource};
use fedlex_jolux::MockSparqlClient;
use fedlex_store::token_bucket::{Acquisition, BucketParams};
use serde_json::{json, Value};
use time::macros::date;

use mcp_reader::{
    register_discovery_tools, register_metadata_tools, register_navigation_tools, ClaimRecord,
    JsonRpcRequest, McpService, QuotaBackend, QuotaError, RateLimiter, Registry, Role,
    StaticAuthResolver, TemporalResolver,
};

// ============================================================
// Fixtures — gespiegelt aus tests/lexicon_projection.rs, damit der
// `tools/call`-Pfad ohne Netzwerk deterministisch durch die Mocks läuft.
// ============================================================

/// Canned SPARQL-Resultat für die Konsolidierungs-Auflösung (cons/date/url).
const CONS_JSON: &str = r#"{
  "head": { "vars": ["cons", "date", "url"] },
  "results": { "bindings": [ {
    "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20260401" },
    "date": { "type": "literal", "value": "2026-04-01" },
    "url": { "type": "uri", "value": "https://fedlex.data.admin.ch/filestore/x/de/xml" }
  } ] }
}"#;

/// Minimales, strukturell echtes AKN-Dokument mit genau einem Artikel `art_1`.
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

/// Leeres, valides SPARQL-JSON für Metadaten-/Discovery-Mocks.
const EMPTY_JSON: &str = r#"{ "head": { "vars": [] }, "results": { "bindings": [] } }"#;

/// Test-Credential und der daran gebundene Validator-Claim (sieht alle Pools).
const TOKEN: &str = "baseline-token";

// ============================================================
// Test-Doubles
// ============================================================

/// Quota-Backend, das jeden Aufruf erlaubt. Phase 1 prüft den Protokoll-
/// Vertrag, nicht die Drosselung (die hat eigene Tests in `quota.rs`).
struct AllowAllQuota;

impl QuotaBackend for AllowAllQuota {
    async fn try_acquire(
        &self,
        _key: &str,
        params: BucketParams,
        _cost: u32,
        _now_ms: u64,
    ) -> Result<Acquisition, QuotaError> {
        Ok(Acquisition {
            allowed: true,
            remaining: params.capacity as i64,
            retry_after_ms: 0,
        })
    }
}

// ============================================================
// Harness — baut den reinen `McpService` ohne Netzwerk (genau die
// Verdrahtung aus `main.rs`, mit Mock-Quellen).
// ============================================================

/// Erzeugt einen einsatzbereiten Dienst mit allen drei Tool-Familien, einem
/// Validator-Credential und einer Always-Allow-Quota.
fn service() -> McpService<StaticAuthResolver, AllowAllQuota> {
    let mut registry = Registry::new();
    let fetcher = Arc::new(AknFetcher::new(
        MockSparqlClient::from_json(CONS_JSON),
        MockXmlSource::new(MINI_ACT),
        8,
    ));
    register_navigation_tools(&mut registry, fetcher);
    register_metadata_tools(
        &mut registry,
        Arc::new(MockSparqlClient::from_json(EMPTY_JSON)),
    );
    register_discovery_tools(
        &mut registry,
        Arc::new(MockSparqlClient::from_json(EMPTY_JSON)),
    );

    let auth = StaticAuthResolver::new().with_credential(
        TOKEN,
        ClaimRecord {
            tenant: "kanzlei-a".into(),
            session: "sess-1".into(),
            role: Role::Validator,
        },
    );

    let limiter = RateLimiter::new(AllowAllQuota);
    let temporal = TemporalResolver::new(date!(2024 - 01 - 01));

    McpService::new(registry, auth, limiter, temporal)
}

/// Baut eine JSON-RPC-Anfrage aus Methode und Parametern.
fn request(id: i64, method: &str, params: Value) -> JsonRpcRequest {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .expect("valide JSON-RPC-Anfrage")
}

/// Ruft `handle` mit gültigem Credential auf und liefert das `result` (panickt,
/// wenn ein `error` zurückkam — der Erfolgspfad ist Teil des Vertrags).
async fn call_ok(method: &str, params: Value) -> Value {
    let resp = service()
        .handle(Some(TOKEN), request(1, method, params), 1_000)
        .await;
    assert!(
        resp.error.is_none(),
        "Methode `{method}` lieferte unerwartet einen JSON-RPC-Fehler: {:?}",
        resp.error
    );
    resp.result.expect("Erfolg muss ein result tragen")
}

// ============================================================
// 1.1 — Baseline-Konformanztest: der `initialize`-Handshake
// ============================================================

#[tokio::test]
async fn initialize_handshake_negotiates_target_revision() {
    let result = call_ok("initialize", json!({})).await;

    // Kontrollierte Pull-Through (Runbook Phase 6.3, ADR-008): Der handshake-lose
    // `initialize` (ohne `protocolVersion`) handelt jetzt die Ziel-Revision
    // `2025-11-25` aus — der vollständige Lifecycle (initialize/initialized/ping)
    // deckt sie. Die handshake-losen Alt-Clients (ansV, syllogismus-fedlex) lesen
    // `protocolVersion` nicht aus und bleiben unberührt; ein explizit `2024-11-05`
    // anfragender Client erhält weiterhin `2024-11-05` (separat getestet im
    // Transport-Modul: `initialize_echoes_supported_client_version`).
    assert_eq!(
        result["protocolVersion"], "2025-11-25",
        "Default-Negotiation muss die Ziel-Revision liefern — Flip ist kontrolliert über ADR-008/Runbook 6.3"
    );

    // Server-Identität, auf die der Smoke-Test und ansV bauen.
    assert_eq!(result["serverInfo"]["name"], "mcp-fedlex-reader");
    assert!(
        result["serverInfo"]["version"].is_string(),
        "serverInfo.version muss gesetzt sein"
    );

    // Capabilities ehrlich und minimal: nur `tools`, sonst nichts.
    assert!(
        result["capabilities"]["tools"].is_object(),
        "capabilities.tools muss als (leeres) Objekt ausgewiesen sein"
    );
    let caps = result["capabilities"]
        .as_object()
        .expect("capabilities ist ein Objekt");
    assert_eq!(
        caps.len(),
        1,
        "Capabilities müssen minimal bleiben (nur `tools`), war: {caps:?}"
    );
    for forbidden in ["resources", "prompts", "logging", "completions"] {
        assert!(
            result["capabilities"].get(forbidden).is_none(),
            "Capability `{forbidden}` darf nicht angekündigt werden, solange nicht implementiert"
        );
    }
}

// ============================================================
// 1.2 — Methoden-Snapshot: tools/list und tools/call (Konsumenten-Vertrag)
// ============================================================

#[tokio::test]
async fn tools_list_entry_shape_is_frozen() {
    let result = call_ok("tools/list", json!({})).await;

    let tools = result["tools"]
        .as_array()
        .expect("tools/list liefert ein Array unter `tools`");
    assert!(!tools.is_empty(), "Validator muss Tools sehen");

    for entry in tools {
        // Eintragsform nach dem additiven Wire-Delta (ADR-008 §A): der
        // MCP-Standardschlüssel `inputSchema` UND der Legacy-Schlüssel `schema`
        // sind beide vorhanden und tragen DENSELBEN Wert. Neue Clients lesen
        // `inputSchema`, der Alt-Client ansV liest `schema` — beide funktionieren
        // bis zur Entfernung des Legacy-Schlüssels (Runbook Phase 9).
        assert!(
            entry["name"].is_string(),
            "Tool-Eintrag ohne string `name`: {entry}"
        );
        assert!(
            entry.get("inputSchema").is_some(),
            "Tool-Eintrag ohne MCP-Standardschlüssel `inputSchema`: {entry}"
        );
        assert!(
            entry.get("schema").is_some(),
            "Legacy-Schlüssel `schema` muss bis Phase 9 erhalten bleiben (Alt-Client ansV): {entry}"
        );
        assert_eq!(
            entry["inputSchema"], entry["schema"],
            "inputSchema und schema müssen denselben Wert tragen (additives Doppel-Emit): {entry}"
        );
    }

    // `read_article` ist Teil des stabilen Tool-Satzes (vom Smoke-Test genutzt).
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(
        names.contains(&"read_article"),
        "read_article muss gelistet sein, war: {names:?}"
    );
}

#[tokio::test]
async fn tools_call_read_article_carries_structural_provenance() {
    // Genau der Aufruf, den scripts/smoke.sh fährt — hier offline gegen die Mocks.
    let result = call_ok(
        "tools/call",
        json!({
            "name": "read_article",
            "arguments": { "eli": "eli/cc/2017/762", "eid": "art_1" },
            "as_of": "2024-01-01"
        }),
    )
    .await;

    // Provenance-Gate (ADR-004): ELI aus dem FRBR-Block, plus Stichtag.
    assert_eq!(
        result["provenance"]["eli"], "eli/cc/2017/762",
        "Provenance.eli muss aus der Quelle stammen, war: {result}"
    );
    assert!(
        !result["provenance"]["valid_as_of"].is_null(),
        "Provenance.valid_as_of muss gesetzt sein, war: {result}"
    );
    // Kein in-band-Fehler in der Nutzlast.
    assert!(
        result.get("error").is_none(),
        "erfolgreicher tools/call darf kein in-band error tragen: {result}"
    );
}

// ============================================================
// Fail-closed-Invarianten des Transports (Teil des Vertrags)
// ============================================================

#[tokio::test]
async fn missing_credential_is_rejected_unauthorized() {
    let resp = service()
        .handle(None, request(1, "tools/list", json!({})), 1_000)
        .await;
    let err = resp
        .error
        .expect("fehlendes Credential muss Fehler liefern");
    assert_eq!(
        err.code, -32001,
        "Auth-Fehlercode (UNAUTHORIZED) ist Teil des Vertrags"
    );
}

#[tokio::test]
async fn invalid_credential_is_rejected_unauthorized() {
    let resp = service()
        .handle(Some("forged"), request(1, "tools/list", json!({})), 1_000)
        .await;
    let err = resp
        .error
        .expect("ungültiges Credential muss Fehler liefern");
    assert_eq!(err.code, -32001);
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let resp = service()
        .handle(Some(TOKEN), request(1, "resources/list", json!({})), 1_000)
        .await;
    let err = resp.error.expect("unbekannte Methode muss Fehler liefern");
    assert_eq!(
        err.code, -32601,
        "unbekannte Methode → METHOD_NOT_FOUND (heutiger Footprint: nur initialize/tools.*)"
    );
}

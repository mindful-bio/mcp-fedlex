//! Gegenseitig authentifizierte Redis-Verbindung (mTLS, ADR-005, Feature
//! `redis-tls`).
//!
//! ADR-001 schützt die Daten im Store, ADR-005 schützt den Weg dorthin. Die
//! einzige interne Cluster-Kante des Readers im Direct-Fetch-Stand (v7.0) ist
//! Reader → Quota-Redis. Diese Schicht trägt das Client-Zertifikat, mit dem
//! sich der Reader gegenüber Redis ausweist, und die CA, gegen die das
//! Server-Zertifikat geprüft wird. Damit ist die Verbindung beidseitig
//! authentifiziert: Redis akzeptiert nur Aufrufer mit gültigem Client-Zert
//! (`tls-auth-clients yes`), und der Reader spricht nur mit einem Server, das
//! die erwartete CA vorweist.
//!
//! Das Zertifikatsmaterial wird als PEM übergeben, nicht aus dem Code geladen.
//! Im Cluster liefert es ein cert-manager-Issuer über ein Volume, das pro
//! Rotation neu geschrieben wird (kurzlebige Identitäten, ADR-005). Diese
//! Schicht kennt nur die Bytes, nicht ihre Herkunft.

use crate::redis_store::RedisError;

/// PEM-Material für eine gegenseitig authentifizierte Redis-Verbindung.
///
/// Alle drei Bestandteile sind Pflicht. Ohne Client-Zertifikat wäre die
/// Verbindung nur server-, nicht gegenseitig authentifiziert; ohne CA könnte
/// der Reader das Server-Zertifikat nicht prüfen und wäre für einen
/// Spoofing-Angriff offen. Die strukturelle Pflicht ist daher Absicht.
#[derive(Clone)]
pub struct RedisTlsConfig {
    /// CA-Wurzelzertifikat (PEM), gegen das das Redis-Server-Zertifikat geprüft wird.
    root_ca_pem: Vec<u8>,
    /// Client-Zertifikat (PEM), mit dem sich der Reader ausweist.
    client_cert_pem: Vec<u8>,
    /// Privater Schlüssel zum Client-Zertifikat (PEM).
    client_key_pem: Vec<u8>,
}

impl RedisTlsConfig {
    /// Baut die Konfiguration aus PEM-Bytes. Leere Eingaben werden hart
    /// abgelehnt, denn eine mTLS-Verbindung ohne eines der drei Stücke wäre
    /// keine mTLS-Verbindung.
    pub fn from_pem(
        root_ca_pem: impl Into<Vec<u8>>,
        client_cert_pem: impl Into<Vec<u8>>,
        client_key_pem: impl Into<Vec<u8>>,
    ) -> Result<Self, RedisError> {
        let root_ca_pem = root_ca_pem.into();
        let client_cert_pem = client_cert_pem.into();
        let client_key_pem = client_key_pem.into();

        if root_ca_pem.is_empty() {
            return Err(RedisError::Tls("leeres CA-Zertifikat".into()));
        }
        if client_cert_pem.is_empty() {
            return Err(RedisError::Tls("leeres Client-Zertifikat".into()));
        }
        if client_key_pem.is_empty() {
            return Err(RedisError::Tls("leerer Client-Schlüssel".into()));
        }

        Ok(Self {
            root_ca_pem,
            client_cert_pem,
            client_key_pem,
        })
    }

    /// Liest die drei PEM-Dateien von der Platte. So liefert sie der
    /// cert-manager im Cluster (gemountetes Secret-Volume).
    pub fn from_files(
        root_ca_path: &str,
        client_cert_path: &str,
        client_key_path: &str,
    ) -> Result<Self, RedisError> {
        let read = |p: &str| -> Result<Vec<u8>, RedisError> {
            std::fs::read(p).map_err(|e| RedisError::Tls(format!("{p}: {e}")))
        };
        Self::from_pem(
            read(root_ca_path)?,
            read(client_cert_path)?,
            read(client_key_path)?,
        )
    }

    /// Übersetzt das Material in die Zertifikatsstruktur der redis-Bibliothek.
    pub(crate) fn to_certificates(&self) -> redis::TlsCertificates {
        redis::TlsCertificates {
            client_tls: Some(redis::ClientTlsConfig {
                client_cert: self.client_cert_pem.clone(),
                client_key: self.client_key_pem.clone(),
            }),
            root_cert: Some(self.root_ca_pem.clone()),
        }
    }
}

impl std::fmt::Debug for RedisTlsConfig {
    /// Verschweigt das Schlüsselmaterial. Ein privater Schlüssel gehört nie in
    /// ein Log (vgl. `Sensitive<T>` in fedlex-core, ADR-001).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisTlsConfig")
            .field("root_ca_pem", &"<redacted>")
            .field("client_cert_pem", &"<redacted>")
            .field("client_key_pem", &"<redacted>")
            .finish()
    }
}

/// Baut einen mTLS-fähigen Redis-Client. Die URL muss `rediss://` sein, sonst
/// würde im Klartext gesprochen und die mTLS-Konfiguration wäre wirkungslos —
/// das wird hart abgelehnt.
pub(crate) fn build_tls_client(
    url: &str,
    tls: &RedisTlsConfig,
) -> Result<redis::Client, RedisError> {
    if !url.starts_with("rediss://") {
        return Err(RedisError::Tls(format!(
            "mTLS verlangt rediss://-Schema, erhalten: {url}"
        )));
    }
    let info = url
        .parse::<redis::ConnectionInfo>()
        .map_err(RedisError::Redis)?;
    redis::Client::build_with_tls(info, tls.to_certificates()).map_err(RedisError::Redis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_material_is_rejected() {
        assert!(RedisTlsConfig::from_pem("", "cert", "key").is_err());
        assert!(RedisTlsConfig::from_pem("ca", "", "key").is_err());
        assert!(RedisTlsConfig::from_pem("ca", "cert", "").is_err());
    }

    #[test]
    fn non_empty_material_builds_config() {
        let cfg = RedisTlsConfig::from_pem("ca", "cert", "key");
        assert!(cfg.is_ok());
    }

    #[test]
    fn debug_redacts_key_material() {
        let cfg =
            RedisTlsConfig::from_pem("ca-bytes", "cert-bytes", "SUPER-SECRET-PRIVATE-KEY").unwrap();

        let rendered = format!("{cfg:?}");
        assert!(
            !rendered.contains("SUPER-SECRET-PRIVATE-KEY"),
            "privates Schlüsselmaterial darf nicht im Debug-Output erscheinen"
        );
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn plaintext_scheme_is_rejected() {
        // Garbage-PEM reicht, denn das Schema wird vor dem TLS-Aufbau geprüft.
        let cfg = RedisTlsConfig::from_pem("ca", "cert", "key").unwrap();
        let err = build_tls_client("redis://mcp-reader-redis:6379", &cfg)
            .expect_err("redis:// (Klartext) muss bei mTLS abgelehnt werden");
        match err {
            RedisError::Tls(msg) => assert!(msg.contains("rediss://")),
            other => panic!("falsche Fehlerart: {other:?}"),
        }
    }
}

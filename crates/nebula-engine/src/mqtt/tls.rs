//! TLS certificate generation for NEBULA MQTT connections.
//!
//! Generates a self-signed CA and server/client certificates at runtime so
//! that all MQTT traffic is encrypted and cannot be observed by packet
//! sniffers like PCAPDroid. The CA certificate is generated per-cluster,
//! meaning PCAPDroid's injected CA certificate is never trusted.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rumqttc::tokio_rustls::rustls::{
    self,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};

/// Holds the CA certificate and key for signing child certificates.
pub struct NebulaCa {
    /// The self-signed CA certificate.
    pub ca_cert: rcgen::Certificate,
    /// The CA private key used to sign child certs.
    pub ca_key: KeyPair,
    /// PEM-encoded CA certificate (for distribution to clients/broker).
    pub ca_cert_pem: String,
}

/// Holds a generated certificate and its private key in both PEM and DER formats.
pub struct CertBundle {
    /// PEM-encoded certificate.
    pub cert_pem: String,
    /// PEM-encoded private key.
    pub key_pem: String,
    /// DER-encoded certificate bytes.
    pub cert_der: Vec<u8>,
    /// DER-encoded private key bytes (PKCS#8).
    pub key_der: Vec<u8>,
}

/// Generate a self-signed CA certificate and key pair.
pub fn generate_ca(common_name: &str) -> Result<NebulaCa> {
    let mut params =
        CertificateParams::new(Vec::new()).context("Failed to create CA cert params")?;
    params.distinguished_name.push(DnType::CommonName, common_name);
    params.distinguished_name.push(DnType::OrganizationName, "NEBULA Network");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::CrlSign,
    ];

    let ca_key = KeyPair::generate().context("Failed to generate CA key pair")?;
    let ca_cert = params.self_signed(&ca_key).context("Failed to self-sign CA cert")?;
    let ca_cert_pem = ca_cert.pem();

    Ok(NebulaCa { ca_cert, ca_key, ca_cert_pem })
}

/// Generate a server certificate signed by the CA.
pub fn generate_server_cert(ca: &NebulaCa, hostname: &str) -> Result<CertBundle> {
    let mut params = CertificateParams::new(vec![hostname.to_string()])
        .context("Failed to create server cert params")?;
    params.distinguished_name.push(DnType::CommonName, hostname);
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;

    let key_pair = KeyPair::generate().context("Failed to generate server key pair")?;
    let cert = params.signed_by(&key_pair, &ca.ca_cert, &ca.ca_key)
        .context("Failed to sign server cert")?;

    Ok(CertBundle {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        cert_der: cert.der().to_vec(),
        key_der: key_pair.serialize_der(),
    })
}

/// Generate a client certificate signed by the CA.
pub fn generate_client_cert(ca: &NebulaCa, node_id: &str) -> Result<CertBundle> {
    let mut params = CertificateParams::new(vec![node_id.to_string()])
        .context("Failed to create client cert params")?;
    params.distinguished_name.push(DnType::CommonName, node_id);
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    params.use_authority_key_identifier_extension = true;

    let key_pair = KeyPair::generate().context("Failed to generate client key pair")?;
    let cert = params.signed_by(&key_pair, &ca.ca_cert, &ca.ca_key)
        .context("Failed to sign client cert")?;

    Ok(CertBundle {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        cert_der: cert.der().to_vec(),
        key_der: key_pair.serialize_der(),
    })
}

/// Create a rustls ClientConfig that trusts only our CA.
pub fn make_client_tls_config(
    ca_cert_pem: &str,
    client_cert: Option<&CertBundle>,
) -> Result<rustls::ClientConfig> {
    let ca_cert_der = pem_to_der(ca_cert_pem).context("Failed to parse CA PEM")?;
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add(CertificateDer::from(ca_cert_der))
        .context("Failed to add CA cert to root store")?;

    let config = if let Some(client) = client_cert {
        let client_cert_der = CertificateDer::from(client.cert_der.clone());
        let client_key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(client.key_der.clone()));
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_auth_cert(vec![client_cert_der], client_key_der)
            .context("Failed to build client TLS config with client auth")?
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    };

    Ok(config)
}

/// Write certificate PEM files to disk and return a rumqttd TlsConfig for the broker.
pub fn make_broker_tls_config(
    ca_pem: &str,
    server_cert: &CertBundle,
    cert_dir: &Path,
) -> Result<rumqttd::TlsConfig> {
    std::fs::create_dir_all(cert_dir)
        .with_context(|| format!("Failed to create cert dir: {}", cert_dir.display()))?;

    let ca_path = cert_dir.join("ca.pem");
    let cert_path = cert_dir.join("server.pem");
    let key_path = cert_dir.join("server-key.pem");

    write_file(&ca_path, ca_pem.as_bytes())?;
    write_file(&cert_path, server_cert.cert_pem.as_bytes())?;
    write_file(&key_path, server_cert.key_pem.as_bytes())?;

    Ok(rumqttd::TlsConfig::Rustls {
        capath: Some(ca_path.to_string_lossy().into_owned()),
        certpath: cert_path.to_string_lossy().into_owned(),
        keypath: key_path.to_string_lossy().into_owned(),
    })
}

/// Convenience: generate a full TLS setup and return a ready-to-use client config.
pub fn generate_tls_setup(
    cluster_name: &str,
    node_id: &str,
) -> Result<(NebulaCa, CertBundle, Arc<rustls::ClientConfig>)> {
    let ca = generate_ca(&format!("{cluster_name} CA"))?;
    let server_cert = generate_server_cert(&ca, "localhost")?;
    let client_cert = generate_client_cert(&ca, node_id)?;
    let tls_config = make_client_tls_config(&ca.ca_cert_pem, Some(&client_cert))?;
    Ok((ca, server_cert, Arc::new(tls_config)))
}

fn pem_to_der(pem_str: &str) -> Result<Vec<u8>> {
    let parsed = pem::parse(pem_str).context("Invalid PEM data")?;
    Ok(parsed.into_contents())
}

fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("Failed to create file: {}", path.display()))?;
    file.write_all(data)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ca() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        assert!(!ca.ca_cert_pem.is_empty());
        assert!(ca.ca_cert_pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn test_generate_server_cert() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let server = generate_server_cert(&ca, "localhost").unwrap();
        assert!(!server.cert_pem.is_empty());
        assert!(!server.key_pem.is_empty());
        assert!(server.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(server.key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_generate_client_cert() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let client = generate_client_cert(&ca, "node-abc-123").unwrap();
        assert!(!client.cert_pem.is_empty());
        assert!(!client.key_pem.is_empty());
        assert!(client.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(client.key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_different_certs_have_different_keys() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let server = generate_server_cert(&ca, "localhost").unwrap();
        let client = generate_client_cert(&ca, "node-1").unwrap();
        assert_ne!(server.key_der, client.key_der);
        assert_ne!(server.cert_der, client.cert_der);
    }

    #[test]
    fn test_make_client_tls_config_no_client_auth() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let config = make_client_tls_config(&ca.ca_cert_pem, None).unwrap();
        let _ = config;
    }

    #[test]
    fn test_make_client_tls_config_with_client_auth() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let client_cert = generate_client_cert(&ca, "test-node").unwrap();
        let config = make_client_tls_config(&ca.ca_cert_pem, Some(&client_cert)).unwrap();
        let _ = config;
    }

    #[test]
    fn test_make_broker_tls_config() {
        let ca = generate_ca("NEBULA Test CA").unwrap();
        let server_cert = generate_server_cert(&ca, "localhost").unwrap();
        let temp_dir = std::env::temp_dir().join("nebula-tls-broker-test");
        let tls_config = make_broker_tls_config(&ca.ca_cert_pem, &server_cert, &temp_dir).unwrap();

        match tls_config {
            rumqttd::TlsConfig::Rustls { capath, certpath, keypath } => {
                assert!(capath.is_some());
                assert!(Path::new(&certpath).exists());
                assert!(Path::new(&keypath).exists());
                assert!(Path::new(&capath.unwrap()).exists());
            }
            #[allow(unreachable_patterns)]
            _ => panic!("Expected Rustls TLS config"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ca_certs_are_unique() {
        let ca1 = generate_ca("CA One").unwrap();
        let ca2 = generate_ca("CA Two").unwrap();
        assert_ne!(ca1.ca_cert_pem, ca2.ca_cert_pem);
    }

    #[test]
    fn test_pem_to_der_roundtrip() {
        let ca = generate_ca("Test").unwrap();
        let der = pem_to_der(&ca.ca_cert_pem).unwrap();
        assert!(!der.is_empty());
        assert_eq!(der, ca.ca_cert.der().as_ref());
    }

    #[test]
    fn test_generate_tls_setup() {
        let (ca, server_cert, client_config) = generate_tls_setup("test-cluster", "node-1").unwrap();
        assert!(!ca.ca_cert_pem.is_empty());
        assert!(!server_cert.cert_pem.is_empty());
        let _ = client_config;
    }
}

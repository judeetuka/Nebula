//! TLS transport layer for nebula-server.

use super::{AddrMaybeCached, SocketOpts, Transport};
use crate::config::TransportConfig;
use crate::helper::tcp_connect_direct;

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::fmt::{self, Debug};
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio_rustls::rustls::{self, ClientConfig, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::{debug, warn};

pub enum TlsStreamWrapper {
    Client(tokio_rustls::client::TlsStream<TcpStream>),
    Server(tokio_rustls::server::TlsStream<TcpStream>),
}

impl Debug for TlsStreamWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(_) => f.write_str("TlsStream(client)"),
            Self::Server(_) => f.write_str("TlsStream(server)"),
        }
    }
}

impl tokio::io::AsyncRead for TlsStreamWrapper {
    fn poll_read(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Client(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            Self::Server(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for TlsStreamWrapper {
    fn poll_write(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Client(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            Self::Server(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Client(s) => std::pin::Pin::new(s).poll_flush(cx),
            Self::Server(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Client(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            Self::Server(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub struct TlsTransport {
    socket_opts: SocketOpts,
    tls_acceptor: TlsAcceptor,
    tls_connector: TlsConnector,
    server_name: ServerName<'static>,
}

impl Debug for TlsTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsTransport")
            .field("server_name", &self.server_name)
            .finish()
    }
}

impl TlsTransport {
    fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
        let file = std::fs::File::open(path).with_context(|| format!("open cert: {path}"))?;
        rustls_pemfile::certs(&mut BufReader::new(file))
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("parse certs: {path}"))
    }

    fn load_key(path: &str) -> Result<PrivateKeyDer<'static>> {
        let file = std::fs::File::open(path).with_context(|| format!("open key: {path}"))?;
        rustls_pemfile::private_key(&mut BufReader::new(file))
            .with_context(|| format!("parse key: {path}"))?
            .ok_or_else(|| anyhow::anyhow!("no private key in: {path}"))
    }

    fn generate_self_signed(hostname: &str) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
        let key_pair = rcgen::KeyPair::generate().context("keygen")?;
        let mut params = rcgen::CertificateParams::new(vec![hostname.to_string()]).context("cert params")?;
        params.distinguished_name.push(rcgen::DnType::CommonName, hostname);
        let cert = params.self_signed(&key_pair).context("self-sign")?;
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(key_pair.serialize_der()).map_err(|e| anyhow::anyhow!("key DER: {e}"))?;
        debug!("Generated self-signed TLS cert for '{hostname}'");
        Ok((vec![cert_der], key_der))
    }
}

#[async_trait]
impl Transport for TlsTransport {
    type Acceptor = TcpListener;
    type RawStream = TcpStream;
    type Stream = TlsStreamWrapper;

    fn new(config: &TransportConfig) -> Result<Self> {
        let tls_cfg = config.tls.as_ref().ok_or_else(|| anyhow::anyhow!("TLS transport requires [transport.tls] config"))?;
        let hostname = tls_cfg.hostname.as_deref().unwrap_or("localhost");

        let (certs, key) = if let Some(cert_path) = &tls_cfg.pkcs12 {
            let key_path = tls_cfg.pkcs12_password.as_ref().map(|p| p.0.as_str()).unwrap_or(cert_path);
            (Self::load_certs(cert_path)?, Self::load_key(key_path)?)
        } else {
            warn!("No TLS certs configured; generating self-signed for '{hostname}'");
            Self::generate_self_signed(hostname)?
        };

        let server_config = ServerConfig::builder().with_no_client_auth().with_single_cert(certs.clone(), key).context("build server config")?;
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

        let mut root_store = rustls::RootCertStore::empty();
        if let Some(ca_path) = &tls_cfg.trusted_root {
            for cert in Self::load_certs(ca_path)? { root_store.add(cert).context("add CA cert")?; }
        } else {
            // Trust our self-signed cert for development
            for cert in &certs { root_store.add(cert.clone()).context("add self-signed root")?; }
        }
        let client_config = ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth();
        let tls_connector = TlsConnector::from(Arc::new(client_config));
        let server_name = ServerName::try_from(hostname.to_string()).map_err(|e| anyhow::anyhow!("invalid SNI: {e}"))?;

        Ok(TlsTransport { socket_opts: SocketOpts::from_cfg(&config.tcp), tls_acceptor, tls_connector, server_name })
    }

    fn hint(_conn: &Self::Stream, _opts: SocketOpts) {}

    async fn bind<T: ToSocketAddrs + Send + Sync>(&self, addr: T) -> Result<Self::Acceptor> {
        Ok(TcpListener::bind(addr).await?)
    }

    async fn accept(&self, a: &Self::Acceptor) -> Result<(Self::RawStream, SocketAddr)> {
        let (stream, addr) = a.accept().await?;
        self.socket_opts.apply(&stream);
        Ok((stream, addr))
    }

    async fn handshake(&self, conn: Self::RawStream) -> Result<Self::Stream> {
        Ok(TlsStreamWrapper::Server(self.tls_acceptor.accept(conn).await.context("TLS server handshake")?))
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        let tcp = tcp_connect_direct(addr).await?;
        self.socket_opts.apply(&tcp);
        Ok(TlsStreamWrapper::Client(self.tls_connector.connect(self.server_name.clone(), tcp).await.context("TLS client handshake")?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn test_generate_self_signed() {
        ensure_crypto_provider();
        let (certs, _key) = TlsTransport::generate_self_signed("localhost").unwrap();
        assert_eq!(certs.len(), 1);
        assert!(!certs[0].is_empty());
    }

    #[test]
    fn test_tls_transport_new_self_signed() {
        ensure_crypto_provider();
        use crate::config::{TcpConfig, TlsConfig, TransportType};
        let config = TransportConfig {
            transport_type: TransportType::Tls,
            tcp: TcpConfig::default(),
            tls: Some(TlsConfig { hostname: Some("localhost".into()), trusted_root: None, pkcs12: None, pkcs12_password: None }),
            noise: None, websocket: None,
        };
        assert!(TlsTransport::new(&config).is_ok());
    }

    #[test]
    fn test_tls_transport_new_missing_config() {
        ensure_crypto_provider();
        use crate::config::{TcpConfig, TransportType};
        let config = TransportConfig {
            transport_type: TransportType::Tls,
            tcp: TcpConfig::default(),
            tls: None, noise: None, websocket: None,
        };
        assert!(TlsTransport::new(&config).is_err());
    }

    #[tokio::test]
    async fn test_tls_roundtrip() {
        ensure_crypto_provider();
        let (certs, key) = TlsTransport::generate_self_signed("localhost").unwrap();
        let sc = ServerConfig::builder().with_no_client_auth().with_single_cert(certs.clone(), key).unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(sc));

        let mut rs = rustls::RootCertStore::empty();
        rs.add(certs[0].clone()).unwrap();
        let cc = ClientConfig::builder().with_root_certificates(rs).with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(cc));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(tcp).await.unwrap();
            let mut buf = [0u8; 64];
            let n = tokio::io::AsyncReadExt::read(&mut tls, &mut buf).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut tls, &buf[..n]).await.unwrap();
        });

        let tcp = TcpStream::connect(addr).await.unwrap();
        let sn = ServerName::try_from("localhost".to_string()).unwrap();
        let mut tls = connector.connect(sn, tcp).await.unwrap();
        let msg = b"nebula-tls";
        tokio::io::AsyncWriteExt::write_all(&mut tls, msg).await.unwrap();
        let mut buf = [0u8; 64];
        let n = tokio::io::AsyncReadExt::read(&mut tls, &mut buf).await.unwrap();
        assert_eq!(&buf[..n], msg);
        server.await.unwrap();
    }
}

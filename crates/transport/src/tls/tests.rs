use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;

use super::*;
use crate::Memory;

const CA: &[u8] = include_bytes!("../../../../fixtures/tls/ca.pem");
const CERT: &[u8] = include_bytes!("../../../../fixtures/tls/cert.pem");
const KEY: &[u8] = include_bytes!("../../../../fixtures/tls/key.pem");

fn client(alpn: &[Alpn]) -> TlsConnector {
    let mut roots = RootCertStore::empty();

    for cert in CertificateDer::pem_slice_iter(CA) {
        roots.add(cert.expect("CA parses")).expect("CA is valid");
    }

    let mut config = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();

    config.alpn_protocols = alpn.iter().map(|alpn| alpn.id().to_vec()).collect();

    TlsConnector::from(Arc::new(config))
}

async fn handshake(
    alpn: &[Alpn],
) -> (
    TlsConn<tokio::io::DuplexStream>,
    tokio_rustls::client::TlsStream<tokio::io::DuplexStream>,
) {
    let config = server_config(CERT, KEY).expect("fixture config");
    let (memory, connector) = Memory::new(4096);
    let mut listener = Tls::new(memory, Arc::new(config))
        .bind(())
        .await
        .expect("bind");
    let raw = connector.connect().await.expect("connect");
    let name = ServerName::try_from("localhost").expect("server name");
    let client = client(alpn);
    let accepted = listener.accept().await.expect("accept");

    let (server, client) = tokio::join!(accepted.establish(), client.connect(name, raw));

    (
        server.expect("server handshake"),
        client.expect("client handshake"),
    )
}

#[tokio::test]
async fn handshake_negotiates_h2_and_carries_bytes() {
    let (mut server, mut client) = handshake(&Alpn::ALL).await;
    let mut buf = [0; 4];

    client.write_all(b"ping").await.expect("write");
    client.flush().await.expect("flush");
    server.read_exact(&mut buf).await.expect("read");

    assert_eq!(&buf, b"ping");
    assert_eq!(server.info().alpn, Some(Alpn::H2));
    assert!(server.info().secure);
}

#[tokio::test]
async fn handshake_honours_client_alpn_choice() {
    let (server, _client) = handshake(&[Alpn::Http11]).await;

    assert_eq!(server.info().alpn, Some(Alpn::Http11));
}

#[test]
fn config_rejects_missing_certificates() {
    let error = server_config(b"", KEY).expect_err("no certificates");

    assert!(matches!(error, TlsError::NoCertificates));
}

#[test]
fn config_reports_unparsable_key() {
    let error = server_config(CERT, b"not a key").expect_err("bad key");

    assert!(matches!(error, TlsError::Pem { ref what, .. } if what == "private key"));
}

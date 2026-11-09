use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

#[tokio::test]
async fn connector_reaches_listener() {
    let (transport, connector) = Memory::new(64);
    let mut listener = transport.bind(()).await.expect("bind");
    let mut client = connector.connect().await.expect("connect");
    let mut server = listener
        .accept()
        .await
        .expect("accept")
        .establish()
        .await
        .expect("establish");
    let mut buf = [0; 4];

    client.write_all(b"ping").await.expect("write");
    server.read_exact(&mut buf).await.expect("read");

    assert_eq!(&buf, b"ping");
    assert_eq!(server.info(), ConnInfo::default());
}

#[tokio::test]
async fn memory_transport_binds_once() {
    let (transport, _connector) = Memory::new(64);

    transport.bind(()).await.expect("first bind");

    let error = transport.bind(()).await.expect_err("second bind fails");

    assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

#[tokio::test]
async fn accepted_connection_carries_bytes_and_peer() {
    let mut listener = Tcp::new()
        .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let mut client = TcpStream::connect(addr).await.expect("connect");
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
    assert_eq!(
        server.info().peer,
        Some(client.local_addr().expect("client addr"))
    );
    assert!(!server.info().secure);
    assert!(server.nodelay().expect("nodelay"));
}

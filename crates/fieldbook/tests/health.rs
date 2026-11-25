use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

#[tokio::test]
async fn serves_health_and_preserves_unknown_routes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, fieldbook::router()).await.unwrap();
    });

    let health = get(address, "/healthz").await;
    let (headers, body) = health.split_once("\r\n\r\n").unwrap();
    assert_eq!(headers.lines().next(), Some("HTTP/1.1 200 OK"));
    assert!(headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("content-type: application/json")));
    assert_eq!(body, r#"{"status":"ok"}"#);

    let unknown = get(address, "/does-not-exist").await;
    assert_eq!(unknown.lines().next(), Some("HTTP/1.1 404 Not Found"));
    server.abort();
}

async fn get(address: SocketAddr, path: &str) -> String {
    timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(address).await.unwrap();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    })
    .await
    .expect("HTTP request timed out")
}

use std::{net::SocketAddr, time::Duration};

use axum::Router;
use fieldbook::storage;
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};

#[tokio::test]
async fn serves_health_and_preserves_unknown_routes() {
    let (address, server) = serve(fieldbook::router(None)).await;
    assert_status(address, "/healthz", "200 OK", r#"{"status":"ok"}"#).await;
    assert_status(
        address,
        "/readyz",
        "503 Service Unavailable",
        r#"{"status":"unavailable"}"#,
    )
    .await;

    let unknown = get(address, "/does-not-exist").await;
    assert_eq!(unknown.lines().next(), Some("HTTP/1.1 404 Not Found"));
    server.abort();
}

#[tokio::test]
#[ignore = "requires an empty disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn readiness_tracks_the_migration_and_the_fixture_is_committed() {
    let url =
        std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set FIELDBOOK_TEST_DATABASE_URL");
    let pool = storage::connect_lazy(url.parse().expect("invalid test database URL"));
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version / 10_000, 16);
    let migrations_exist: bool =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!migrations_exist, "the test requires a fresh database");

    let (address, server) = serve(fieldbook::router(Some(pool.clone()))).await;
    assert_status(
        address,
        "/readyz",
        "503 Service Unavailable",
        r#"{"status":"unavailable"}"#,
    )
    .await;
    assert_status(address, "/healthz", "200 OK", r#"{"status":"ok"}"#).await;

    storage::migrate(&pool).await.unwrap();
    storage::migrate(&pool).await.unwrap();
    assert_status(address, "/readyz", "200 OK", r#"{"status":"ok"}"#).await;

    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/household.json")).unwrap();
    let label = format!(
        "Fixture household {}",
        fixture["households"][0].as_str().unwrap()
    );
    let id: String =
        sqlx::query_scalar("INSERT INTO households (label) VALUES ($1) RETURNING id::text")
            .bind(&label)
            .fetch_one(&pool)
            .await
            .unwrap();
    server.abort();
    pool.close().await;

    let reconnected = storage::connect_lazy(url.parse().unwrap());
    let saved: String = sqlx::query_scalar("SELECT label FROM households WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(&reconnected)
        .await
        .unwrap();
    assert_eq!(saved, label);
}

async fn serve(router: Router) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (address, server)
}

async fn assert_status(address: SocketAddr, path: &str, status: &str, expected: &str) {
    let response = get(address, path).await;
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    assert_eq!(
        headers.lines().next().unwrap(),
        format!("HTTP/1.1 {status}")
    );
    assert!(headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("content-type: application/json")));
    assert_eq!(body, expected);
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

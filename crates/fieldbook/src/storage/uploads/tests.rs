use std::{fs, net::SocketAddr, path::PathBuf};

use axum::Router;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::random;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use uuid::Uuid;

use super::{files::original_key, store::clean_one, PrivateMedia};
use crate::{identity, storage};

const HOST: &str = "localhost:4192";
const ORIGIN: &str = "http://localhost:4192";
const SECRET: &str = "fictional B3.1 suite session secret";

struct TestMedia(PathBuf);

impl TestMedia {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("fieldbook-upload-{:032x}", random::<u128>()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn stage(&self, id: &str) -> PathBuf {
        self.0.join(format!("staging-{id}"))
    }

    fn original(&self, id: &str) -> PathBuf {
        self.0.join(format!("original-{id}"))
    }
}

impl Drop for TestMedia {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Session {
    cookie: String,
    csrf: String,
    actor: Uuid,
}

async fn session(pool: &PgPool, household: Uuid, role: &str) -> Session {
    session_named(pool, household, role, "Fictional uploader").await
}

async fn session_named(pool: &PgPool, household: Uuid, role: &str, name: &str) -> Session {
    let raw = URL_SAFE_NO_PAD.encode(random::<[u8; 32]>());
    let username = format!("upload-{:016x}", random::<u64>());
    let actor = identity::provision_actor(pool, household, &username, name, role)
        .await
        .unwrap();
    let mac = |purpose: &[u8]| {
        let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(purpose);
        mac.update(&[0]);
        mac.update(raw.as_bytes());
        mac.finalize().into_bytes()
    };
    sqlx::query(
        "INSERT INTO sessions (token_hash, household_id, actor_id, idle_until, expires_at)
         VALUES ($1, $2, $3, now() + interval '15 minutes', now() + interval '1 hour')",
    )
    .bind(&mac(b"session")[..])
    .bind(household)
    .bind(actor)
    .execute(pool)
    .await
    .unwrap();
    Session {
        cookie: format!("__Host-fieldbook={raw}"),
        csrf: URL_SAFE_NO_PAD.encode(mac(b"csrf")),
        actor,
    }
}

async fn household(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO households (label) VALUES ('Fictional B3.1') RETURNING id")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn serve(router: Router) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (address, task)
}

async fn request(
    address: SocketAddr,
    session: &Session,
    method: &str,
    path: &str,
    content_type: &str,
    body: &[u8],
    csrf: bool,
) -> (u16, Value) {
    let token = if csrf {
        format!("X-CSRF-Token: {}\r\n", session.csrf)
    } else {
        String::new()
    };
    let (status, _, bytes) = request_bytes(
        address,
        session,
        method,
        path,
        &format!("{token}Content-Type: {content_type}\r\n"),
        body,
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn request_bytes(
    address: SocketAddr,
    session: &Session,
    method: &str,
    path: &str,
    extra_headers: &str,
    body: &[u8],
) -> (u16, String, Vec<u8>) {
    let url = format!(
        "{method} {path} HTTP/1.1\r\nHost: {HOST}\r\nOrigin: {ORIGIN}\r\nCookie: {}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        session.cookie, body.len()
    );
    let mut connection = TcpStream::connect(address).await.unwrap();
    connection.write_all(url.as_bytes()).await.unwrap();
    connection.write_all(body).await.unwrap();
    let mut response = Vec::new();
    connection.read_to_end(&mut response).await.unwrap();
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8(response[..split].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(headers.contains("cache-control: no-store, private"));
    (status, headers, response.split_off(split + 4))
}

macro_rules! call {
    ($address:expr, $session:expr, $method:literal, $path:expr) => {
        request(
            $address,
            $session,
            $method,
            $path,
            "application/json",
            b"",
            true,
        )
        .await
    };
    ($address:expr, $session:expr, $method:literal, $path:expr, $json:expr) => {
        request(
            $address,
            $session,
            $method,
            $path,
            "application/json",
            &$json.to_string().into_bytes(),
            true,
        )
        .await
    };
}

mod shares;

async fn part(
    address: SocketAddr,
    session: &Session,
    path: &str,
    offset: usize,
    bytes: &[u8],
) -> (u16, Value) {
    request(
        address,
        session,
        "PUT",
        &format!("{path}/parts/{offset}"),
        "application/octet-stream",
        bytes,
        true,
    )
    .await
}

async fn assert_unpublished(pool: &PgPool, media: &TestMedia, id: &str) {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assets WHERE id = $1)")
        .bind(id.parse::<Uuid>().unwrap())
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(!exists);
    assert!(!media.original(id).exists());
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn private_copy_resume_and_publish() {
    let url = std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set disposable database URL");
    let pool = storage::connect_lazy(url.parse().unwrap());
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version / 10_000, 16);
    storage::migrate(&pool).await.unwrap();

    let household_a = household(&pool).await;
    let uploader = session(&pool, household_a, "learner").await;
    let guardian = session(&pool, household_a, "guardian").await;
    let guest = session(&pool, household_a, "guest").await;
    let outsider = session(&pool, household(&pool).await, "learner").await;
    let media = TestMedia::new();
    let private = PrivateMedia::new(media.0.clone());
    let router = || {
        identity::router_with_media(
            storage::connect_lazy(url.parse().unwrap()),
            ORIGIN,
            "localhost",
            SECRET,
            PathBuf::from("web/dist"),
            Some(private.clone()),
        )
        .unwrap()
    };
    let (address, server) = serve(router()).await;
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../../fixtures/assets.json")).unwrap();
    let fixture = &fixtures[0];
    assert_eq!(fixture["id"], "photo");
    let bytes = include_bytes!("../../../../../fixtures/media/practice-photo.jpg");
    let manifest = json!({
        "submission_key": Uuid::from_u128(1),
        "label": "Fictional practice photo",
        "byte_count": fixture["byte_length"],
        "sha256": fixture["sha256"],
        "media_type": fixture["mime"],
    });
    let (status, upload) = call!(address, &uploader, "POST", "/api/v1/uploads", manifest);
    assert_eq!(status, 201);
    let id = upload["id"].as_str().unwrap();
    let path = format!("/api/v1/uploads/{id}");
    let finalize = format!("{path}/finalize");
    assert_eq!(upload["received"], 0);
    assert_eq!(upload["asset_id"], Value::Null);
    let photo_only = json!({"submission_key": Uuid::from_u128(2), "asset_ids": [id]});
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/moments", photo_only).0,
        404
    );
    assert_eq!(call!(address, &uploader, "POST", &finalize).0, 409);
    assert_eq!(
        part(address, &uploader, &path, 10, &bytes[10..20]).await.0,
        409
    );
    for caller in [&guardian, &outsider, &guest] {
        assert_eq!(call!(address, caller, "GET", &path).0, 404);
        assert_eq!(part(address, caller, &path, 0, &bytes[..4]).await.0, 404);
        assert_eq!(call!(address, caller, "POST", &finalize).0, 404);
    }
    assert_eq!(
        request(
            address,
            &uploader,
            "POST",
            &finalize,
            "application/json",
            b"",
            false
        )
        .await
        .0,
        403
    );

    let halfway = bytes.len() / 2;
    let (first, replay) = tokio::join!(
        part(address, &uploader, &path, 0, &bytes[..halfway]),
        part(address, &uploader, &path, 0, &bytes[..halfway]),
    );
    assert_eq!(first, replay);
    assert_eq!(first.0, 200);
    assert_eq!(first.1["received"], halfway);
    assert_eq!(part(address, &uploader, &path, 0, b"changed").await.0, 409);
    assert_eq!(call!(address, &uploader, "POST", &finalize).0, 409);
    assert_unpublished(&pool, &media, id).await;
    server.abort();
    let _ = server.await;

    // Simulate bytes that reached disk before their offset transaction was lost.
    fs::write(
        media.stage(id),
        [&bytes[..halfway], b"uncommitted tail"].concat(),
    )
    .unwrap();
    let (address, server) = serve(router()).await;
    assert_eq!(
        call!(address, &uploader, "GET", &path).1["received"],
        halfway
    );
    let (_, resumed) = part(address, &uploader, &path, halfway, &bytes[halfway..]).await;
    assert_eq!(resumed["received"], bytes.len());
    assert_eq!(resumed["asset_id"], Value::Null);
    assert_unpublished(&pool, &media, id).await;

    // Simulate a crash after the durable link but before its database commit.
    fs::hard_link(media.stage(id), media.original(id)).unwrap();
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/moments", photo_only).0,
        404
    );
    assert_eq!(
        call!(address, &uploader, "GET", &format!("/assets/original-{id}")).0,
        404
    );
    let (first, replay) = tokio::join!(
        async { call!(address, &uploader, "POST", &finalize) },
        async { call!(address, &uploader, "POST", &finalize) },
    );
    assert_eq!(first, replay);
    assert_eq!(first.0, 200);
    assert_eq!(first.1["asset_id"], id);
    assert!(clean_one(&pool, private.clone()).await.unwrap());
    assert!(!media.stage(id).exists());

    let (key, count, hash, author): (String, i64, Vec<u8>, Uuid) = sqlx::query_as(
        "SELECT storage_key, byte_count, sha256, uploader_id FROM assets WHERE id = $1",
    )
    .bind(id.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(key, original_key(id.parse().unwrap()));
    assert_eq!(count as usize, bytes.len());
    assert_eq!(author, uploader.actor);
    let saved = fs::read(media.original(id)).unwrap();
    let digest = Sha256::digest(&saved);
    assert_eq!(saved, bytes);
    assert_eq!(hash, digest.to_vec());
    assert_eq!(format!("{digest:x}"), fixture["sha256"].as_str().unwrap());
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/moments", photo_only).1["asset_ids"],
        json!([id])
    );
    assert_eq!(
        part(address, &uploader, &path, halfway, &bytes[halfway..])
            .await
            .0,
        200
    );
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/uploads", manifest).1,
        first.1
    );
    let mut different_manifest = manifest.clone();
    different_manifest["label"] = json!("Changed manifest");
    assert_eq!(
        call!(
            address,
            &uploader,
            "POST",
            "/api/v1/uploads",
            different_manifest
        )
        .0,
        409
    );

    let mut corrupt_manifest = manifest.clone();
    corrupt_manifest["submission_key"] = json!(Uuid::from_u128(3));
    let (status, bad) = call!(
        address,
        &uploader,
        "POST",
        "/api/v1/uploads",
        corrupt_manifest
    );
    assert_eq!(status, 201);
    let bad_id = bad["id"].as_str().unwrap();
    let bad_path = format!("/api/v1/uploads/{bad_id}");
    let mut corrupt = bytes.to_vec();
    corrupt[halfway] ^= 1;
    assert_eq!(
        part(address, &uploader, &bad_path, 0, &corrupt).await.0,
        200
    );
    let rejected = call!(address, &uploader, "POST", &format!("{bad_path}/finalize"));
    assert_eq!(
        rejected,
        (422, json!({"error": "upload_checksum_mismatch"}))
    );
    assert_unpublished(&pool, &media, bad_id).await;
    sqlx::query("UPDATE private_uploads SET staging_expires_at = now() WHERE id = $1")
        .bind(bad_id.parse::<Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(call!(address, &uploader, "GET", &bad_path).0, 404);
    assert!(clean_one(&pool, private.clone()).await.unwrap());
    assert!(!media.stage(bad_id).exists());
    assert!(!clean_one(&pool, private.clone()).await.unwrap());
    assert_eq!(fs::read_dir(&media.0).unwrap().count(), 1);
    println!("fixture photo, asset {id}, original {count} bytes, sha256 {digest:x}");
    let mut wrong_type = manifest.clone();
    wrong_type["submission_key"] = json!(Uuid::from_u128(4));
    wrong_type["media_type"] = json!("application/pdf");
    let (_, bad) = call!(address, &uploader, "POST", "/api/v1/uploads", wrong_type);
    let bad_id = bad["id"].as_str().unwrap();
    let bad_path = format!("/api/v1/uploads/{bad_id}");
    assert_eq!(part(address, &uploader, &bad_path, 0, bytes).await.0, 200);
    assert_eq!(
        call!(address, &uploader, "POST", &format!("{bad_path}/finalize")),
        (422, json!({"error": "upload_type_mismatch"}))
    );
    assert_unpublished(&pool, &media, bad_id).await;
    wrong_type["media_type"] = json!("text/html");
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/uploads", wrong_type),
        (422, json!({"error": "unsupported_upload_type"}))
    );
    let mut too_large = manifest.clone();
    too_large["byte_count"] = json!(8_i64 * 1024 * 1024 * 1024 + 1);
    assert_eq!(
        call!(address, &uploader, "POST", "/api/v1/uploads", too_large),
        (422, json!({"error": "invalid_upload_manifest"}))
    );

    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for fixture in &fixtures.as_array().unwrap()[1..] {
        let original = fs::read(fixture_root.join(fixture["path"].as_str().unwrap())).unwrap();
        let manifest = json!({
            "submission_key": Uuid::from_u128(random()),
            "label": fixture["id"],
            "byte_count": fixture["byte_length"],
            "sha256": fixture["sha256"],
            "media_type": fixture["mime"],
        });
        let (status, upload) = call!(address, &uploader, "POST", "/api/v1/uploads", manifest);
        assert_eq!(status, 201);
        let id = upload["id"].as_str().unwrap();
        let path = format!("/api/v1/uploads/{id}");
        assert_eq!(part(address, &uploader, &path, 0, &original).await.0, 200);
        assert_eq!(
            call!(address, &uploader, "POST", &format!("{path}/finalize")).1["asset_id"],
            id
        );
        assert!(clean_one(&pool, private.clone()).await.unwrap());
        let saved = fs::read(media.original(id)).unwrap();
        assert_eq!(saved, original);
        let digest = Sha256::digest(&saved);
        assert_eq!(format!("{digest:x}"), fixture["sha256"].as_str().unwrap());
        println!(
            "fixture {}, asset {id}, original {} bytes, sha256 {digest:x}",
            fixture["id"],
            saved.len()
        );
    }
    server.abort();
}

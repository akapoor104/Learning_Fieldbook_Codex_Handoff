use super::*;

const PHOTO: &[u8] = include_bytes!("../../../../../../fixtures/media/practice-photo.jpg");
const LETTER: &[u8] = include_bytes!("../../../../../../fixtures/media/teacher-note.pdf");

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn named_guest_gets_only_pinned_material_and_revocation_survives_restart() {
    let url = std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set disposable database URL");
    let pool = storage::connect_lazy(url.parse().unwrap());
    storage::migrate(&pool).await.unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../../../fixtures/household.json")).unwrap();
    let home = household(&pool).await;
    let other_home = household(&pool).await;
    let learner = session_named(&pool, home, "learner", "Fictional learner").await;
    let guardian = session_named(&pool, home, "guardian", "Fictional parent").await;
    let guest = session(&pool, home, fixture["actors"][3]["role"].as_str().unwrap()).await;
    let wrong_guest = session(&pool, home, "guest").await;
    let outsider = session(&pool, other_home, "guest").await;
    let media = TestMedia::new();
    let router = || {
        identity::router_with_media(
            storage::connect_lazy(url.parse().unwrap()),
            ORIGIN,
            "localhost",
            SECRET,
            PathBuf::from("web/dist"),
            Some(PrivateMedia::new(media.0.clone())),
        )
        .unwrap()
    };
    let (address, server) = serve(router()).await;

    let photo = upload(address, &guardian, "photo.jpg", "image/jpeg", PHOTO).await;
    let letter = upload(address, &guardian, "teacher.pdf", "application/pdf", LETTER).await;
    let later_photo = upload(address, &guardian, "later.jpg", "image/jpeg", PHOTO).await;
    let text = &fixture["highlights"][0]["visible_text"];
    let (status, highlight) = call!(
        address,
        &guardian,
        "POST",
        "/api/v1/moments",
        json!({
            "submission_key": uuid(&pool).await, "learner_id": learner.actor,
            "scribed": true, "note": text, "asset_ids": [letter]
        })
    );
    assert_eq!(status, 201, "{highlight}");
    let moment = highlight["id"].as_str().unwrap();
    let diary = seed_diary(&pool, home, learner.actor, &media)
        .await
        .to_string();
    assert_eq!(
        request_bytes(
            address,
            &learner,
            "GET",
            &format!("/diary/media/{diary}"),
            "",
            b""
        )
        .await
        .0,
        200
    );
    let add_photo = |asset: &str| format!("/api/v1/family/photo-inbox/{}/{asset}", learner.actor);
    let match_photo = |asset: &str| format!("/api/v1/family/photo-inbox/{asset}/match/{moment}");
    for asset in [&photo, &later_photo] {
        assert_eq!(
            call!(address, &guardian, "PUT", &add_photo(asset), json!({})).0,
            204
        );
    }
    assert_eq!(
        call!(address, &guardian, "POST", &match_photo(&photo)).0,
        204
    );

    let id = uuid(&pool).await;
    let path = format!("/api/v1/shares/{id}");
    let accept = format!("{path}/accept");
    let download = |asset: &str| format!("/shares/{id}/media/{asset}");
    let confirmation = format!("/api/v1/moments/{moment}/confirmation");
    let selection = json!({
        "guest_id": guest.actor, "moment_id": moment, "revision": 1,
        "include_text": true, "original_asset_ids": [&photo]
    });
    assert_eq!(call!(address, &guardian, "PUT", &path, selection).0, 422);
    assert_eq!(
        call!(
            address,
            &learner,
            "POST",
            &confirmation,
            json!({"expected_revision": 1})
        )
        .0,
        200
    );
    let withdrawn_path = format!("/api/v1/shares/{}", uuid(&pool).await);
    let (status, withdrawn) = call!(address, &guardian, "PUT", &withdrawn_path, selection);
    assert_eq!(status, 201);
    assert_eq!(
        call!(
            address,
            &learner,
            "DELETE",
            &confirmation,
            json!({"expected_revision": 1})
        )
        .0,
        200
    );
    assert_eq!(
        call!(
            address,
            &learner,
            "POST",
            &confirmation,
            json!({"expected_revision": 1})
        )
        .0,
        200
    );
    assert_eq!(
        call!(
            address,
            &guest,
            "POST",
            &format!("{withdrawn_path}/accept"),
            json!({"token": withdrawn["token"]})
        )
        .0,
        404
    );

    let mut invalid = selection.clone();
    invalid["guest_id"] = json!(outsider.actor);
    assert_eq!(call!(address, &guardian, "PUT", &path, invalid).0, 404);
    invalid = selection.clone();
    invalid["original_asset_ids"] = json!([&photo, &diary]);
    assert_eq!(call!(address, &guardian, "PUT", &path, invalid).0, 404);
    invalid = selection.clone();
    invalid["revision"] = json!(2);
    assert_eq!(call!(address, &guardian, "PUT", &path, invalid).0, 409);
    assert_eq!(
        request(
            address,
            &guardian,
            "PUT",
            &path,
            "application/json",
            selection.to_string().as_bytes(),
            false
        )
        .await
        .0,
        403
    );

    let (status, invitation) = call!(address, &guardian, "PUT", &path, selection);
    assert_eq!(status, 201, "{invitation}");
    assert_eq!(invitation["id"], json!(id));
    assert_eq!(
        call!(address, &guardian, "PUT", &path, selection),
        (200, invitation.clone())
    );
    assert_eq!(call!(address, &guest, "GET", &path).0, 404);
    let claim = json!({"token": invitation["token"]});
    assert_eq!(call!(address, &wrong_guest, "POST", &accept, claim).0, 404);
    assert_eq!(
        call!(address, &guest, "POST", &accept, json!({"token": "wrong"})).0,
        404
    );
    let (first, second) = tokio::join!(
        async { call!(address, &guest, "POST", &accept, claim) },
        async { call!(address, &guest, "POST", &accept, claim) }
    );
    let mut accept_statuses = [first.0, second.0];
    accept_statuses.sort_unstable();
    assert_eq!(accept_statuses, [204, 404]);

    assert_eq!(
        call!(address, &guardian, "POST", &match_photo(&later_photo)).0,
        204
    );
    let moment_path = format!("/api/v1/moments/{moment}");
    assert_eq!(
        call!(
            address,
            &learner,
            "PATCH",
            &moment_path,
            json!({
                "expected_revision": 1, "note": "New revision with extra details"
            })
        )
        .0,
        200
    );
    let (status, shared) = call!(address, &guest, "GET", &path);
    assert_eq!(status, 200, "{shared}");
    assert_eq!(shared["revision"], 1);
    assert_eq!(&shared["note"], text);
    assert_eq!(shared["voice"], "scribed");
    assert_eq!(shared["learner_name"], "Fictional learner");
    assert_eq!(shared["recorder_name"], "Fictional parent");
    assert_eq!(
        shared["originals"],
        json!([
            {"id": photo, "label": "photo.jpg", "representation": "original", "uploader_name": "Fictional parent"}
        ])
    );
    for actor in [&wrong_guest, &outsider] {
        assert_eq!(call!(address, actor, "GET", &path).0, 404);
        assert_eq!(call!(address, actor, "GET", &download(&photo)).0, 404);
    }
    for url in [
        download(&letter),
        download(&later_photo),
        download(&diary),
        moment_path,
        format!("/media/{photo}"),
        format!("/diary/media/{diary}"),
        "/api/v1/moments".to_owned(),
        "/api/v1/diary".to_owned(),
    ] {
        assert_eq!(
            call!(address, &guest, "GET", &url),
            (404, json!({"error": "not_found"})),
            "{url}"
        );
    }
    let file = download(&photo);
    let (status, headers, bytes) = request_bytes(address, &guest, "GET", &file, "", b"").await;
    assert_eq!((status, bytes.as_slice()), (200, PHOTO));
    assert!(headers.contains("filename*=UTF-8''photo.jpg"));
    let (status, _, bytes) =
        request_bytes(address, &guest, "GET", &file, "Range: bytes=0-7\r\n", b"").await;
    assert_eq!((status, bytes.as_slice()), (206, &PHOTO[..8]));
    assert_eq!(
        request_bytes(address, &guest, "HEAD", &file, "", b"")
            .await
            .0,
        200
    );

    // A separate file-only share tests recovery while the first share is still active.
    let rematch_id = uuid(&pool).await;
    let rematch_path = format!("/api/v1/shares/{rematch_id}");
    let rematch_file = format!("/shares/{rematch_id}/media/{later_photo}");
    let mut file_only = selection.clone();
    file_only["revision"] = json!(2);
    file_only["include_text"] = json!(false);
    file_only["original_asset_ids"] = json!([later_photo]);
    let (status, invite) = call!(address, &guardian, "PUT", &rematch_path, file_only);
    assert_eq!(status, 201);
    assert_eq!(
        call!(
            address,
            &guest,
            "POST",
            &format!("{rematch_path}/accept"),
            json!({"token": invite["token"]})
        )
        .0,
        204
    );
    let (status, shared) = call!(address, &guest, "GET", &rematch_path);
    assert_eq!(status, 200);
    assert_eq!(shared["note"], Value::Null);
    assert_eq!(
        request_bytes(address, &guest, "HEAD", &rematch_file, "", b"")
            .await
            .0,
        200
    );
    let match_later_photo = match_photo(&later_photo);
    assert_eq!(
        call!(address, &guardian, "DELETE", &match_later_photo).0,
        204
    );
    assert_eq!(call!(address, &guardian, "POST", &match_later_photo).0, 204);
    assert_eq!(
        call!(address, &guest, "GET", &rematch_path).1["originals"],
        json!([])
    );
    assert_hidden_download(address, &guest, &rematch_file).await;
    sqlx::query(
        "UPDATE relative_shares SET access_expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(rematch_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(call!(address, &guest, "GET", &rematch_path).0, 404);

    assert_eq!(
        request_bytes(address, &guest, "HEAD", &file, "", b"")
            .await
            .0,
        200
    );
    assert_eq!(call!(address, &guardian, "DELETE", &path).0, 204);
    assert_hidden_download(address, &guest, &file).await;
    server.abort();

    let (address, server) = serve(router()).await;
    assert_eq!(call!(address, &guest, "GET", &path).0, 404);
    assert_hidden_download(address, &guest, &file).await;
    assert_eq!(call!(address, &guest, "POST", &accept, claim).0, 404);
    assert_eq!(call!(address, &guardian, "PUT", &path, selection).0, 409);
    assert_eq!(call!(address, &guardian, "DELETE", &path).0, 204);
    let (status, _, bytes) = request_bytes(
        address,
        &guardian,
        "GET",
        &format!("/media/{photo}"),
        "",
        b"",
    )
    .await;
    assert_eq!((status, bytes.as_slice()), (200, PHOTO));

    // A separate file-only invitation expires without exposing text or a filename.
    file_only["original_asset_ids"] = json!([photo]);
    let new_id = uuid(&pool).await;
    let new_path = format!("/api/v1/shares/{new_id}");
    let (status, invite) = call!(address, &guardian, "PUT", &new_path, file_only);
    assert_eq!(status, 201);
    sqlx::query("UPDATE relative_shares SET invitation_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(new_id).execute(&pool).await.unwrap();
    assert_eq!(
        call!(
            address,
            &guest,
            "POST",
            &format!("{new_path}/accept"),
            json!({
                "token": invite["token"]
            })
        )
        .0,
        404
    );
    assert_eq!(call!(address, &guest, "GET", &new_path).0, 404);
    assert_hidden_download(address, &guest, &format!("/shares/{new_id}/media/{photo}")).await;

    println!("fixture hl1={moment} photo={photo} letter={letter} diary_original={diary} revoked={id} expired={new_id}");
    server.abort();
}

async fn uuid(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT gen_random_uuid()")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn upload(
    address: SocketAddr,
    actor: &Session,
    label: &str,
    kind: &str,
    bytes: &[u8],
) -> String {
    let digest = format!("{:x}", Sha256::digest(bytes));
    let (status, staged) = call!(
        address,
        actor,
        "POST",
        "/api/v1/uploads",
        json!({
            "submission_key": uuid_from_digest_and_label(label), "label": label, "media_type": kind,
            "byte_count": bytes.len(), "sha256": digest
        })
    );
    assert_eq!(status, 201, "{staged}");
    let id = staged["id"].as_str().unwrap();
    let path = format!("/api/v1/uploads/{id}");
    assert_eq!(part(address, actor, &path, 0, bytes).await.0, 200);
    assert_eq!(
        call!(address, actor, "POST", &format!("{path}/finalize")).0,
        200
    );
    id.to_owned()
}

fn uuid_from_digest_and_label(label: &str) -> Uuid {
    let digest = Sha256::digest(label);
    Uuid::from_bytes(digest[..16].try_into().unwrap())
}

async fn seed_diary(pool: &PgPool, household: Uuid, learner: Uuid, media: &TestMedia) -> Uuid {
    let asset = uuid(pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO assets (id, household_id, uploader_id, label, storage_key, byte_count, sha256, scope_owner)
         VALUES ($1, $2, $3, 'diary-private.jpg', $4, $5, $6, $3)",
    )
    .bind(asset).bind(household).bind(learner).bind(original_key(asset))
    .bind(PHOTO.len() as i64).bind(&Sha256::digest(PHOTO)[..])
    .execute(&mut *tx).await.unwrap();
    let moment: Uuid = sqlx::query_scalar(
        "INSERT INTO moments (household_id, learner_id, recorder_id, submission_key,
                              submission_hash, voice, visibility)
         VALUES ($1, $2, $2, gen_random_uuid(), $3, 'student', 'diary') RETURNING id",
    )
    .bind(household)
    .bind(learner)
    .bind(&[0u8; 32][..])
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note)
         VALUES ($1, $2, 1, $3, 'Diary-only fixture')",
    )
    .bind(moment)
    .bind(household)
    .bind(learner)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moment_assets (moment_id, household_id, asset_id, position, scope_owner)
         VALUES ($1, $2, $3, 0, $4)",
    )
    .bind(moment)
    .bind(household)
    .bind(asset)
    .bind(learner)
    .execute(&mut *tx)
    .await
    .unwrap();
    fs::write(media.original(&asset.to_string()), PHOTO).unwrap();
    tx.commit().await.unwrap();
    asset
}

async fn assert_hidden_download(address: SocketAddr, guest: &Session, path: &str) {
    for (method, headers) in [("GET", ""), ("GET", "Range: bytes=0-7\r\n"), ("HEAD", "")] {
        let (status, headers, _) = request_bytes(address, guest, method, path, headers, b"").await;
        assert_eq!(status, 404);
        assert!(!headers.contains("content-disposition"));
    }
}

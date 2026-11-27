use super::{store, Actor, CreateMoment, EditMoment, Failure};
use crate::{identity::provision_actor, storage};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

async fn household(pool: &PgPool, label: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO households (label) VALUES ($1) RETURNING id")
        .bind(label)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn actor(pool: &PgPool, household: Uuid, username: &str, role: &str) -> Actor {
    let alias = format!(
        "journal-{}-{username}",
        &household.simple().to_string()[..8]
    );
    let id = provision_actor(pool, household, &alias, username, role)
        .await
        .unwrap();
    sqlx::query_as("SELECT * FROM actors WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn submission(key: u128, note: Option<&str>) -> CreateMoment {
    CreateMoment {
        submission_key: Uuid::from_u128(key),
        learner_id: None,
        scribed: false,
        note: note.map(str::to_owned),
        activity_date: None,
        asset_ids: vec![],
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn journal_retry_and_revision_recovery() {
    let url = std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set disposable database URL");
    let pool = storage::connect_lazy(url.parse().unwrap());
    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version / 10_000, 16);
    storage::migrate(&pool).await.unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../fixtures/household.json")).unwrap();
    let household_a = household(&pool, "Fictional journal A").await;
    let household_b = household(&pool, "Fictional journal B").await;
    let learner = actor(
        &pool,
        household_a,
        "learner",
        fixture["actors"][0]["role"].as_str().unwrap(),
    )
    .await;
    let guardian = actor(
        &pool,
        household_a,
        "parent-one",
        fixture["actors"][1]["role"].as_str().unwrap(),
    )
    .await;
    let other = actor(&pool, household_b, "other-learner", "learner").await;
    let original = "I lined up the edges so I could compare both pieces.";
    let command = submission(1, Some(original));
    let (first, second) = tokio::join!(
        store::create(&pool, &learner, &command),
        store::create(&pool, &learner, &command)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.note.as_deref(), Some(original));
    assert_eq!(first.asset_ids, Vec::<Uuid>::new());
    assert_eq!(first.activity_date, None);
    assert_eq!(first.voice, "student");
    assert_eq!(first.recorder_id, learner.id);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM moments WHERE household_id = $1")
        .bind(household_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(matches!(
        store::create(
            &pool,
            &learner,
            &submission(1, Some("A different sentence."))
        )
        .await,
        Err(Failure::Conflict)
    ));
    assert!(matches!(
        store::create(&pool, &learner, &submission(2, Some(" \t "))).await,
        Err(Failure::Validation("note_or_asset_required"))
    ));
    assert!(matches!(
        store::read(&pool, &other, first.id).await,
        Err(Failure::Missing)
    ));

    let update = EditMoment {
        expected_revision: 1,
        note: Some("I compared both pieces after lining up their edges.".into()),
        activity_date: Some("2026-01-10".into()),
    };
    assert!(matches!(
        store::edit(&pool, &guardian, first.id, &update).await,
        Err(Failure::Missing)
    ));
    let revised = store::edit(&pool, &learner, first.id, &update)
        .await
        .unwrap();
    assert_eq!(revised.revision, 2);
    let stale = EditMoment {
        expected_revision: 1,
        note: Some("Words from the older tab.".into()),
        activity_date: None,
    };
    match store::edit(&pool, &learner, first.id, &stale).await {
        Err(Failure::Stale { saved, submitted }) => {
            assert_eq!(saved["note"], update.note.unwrap());
            assert_eq!(submitted["note"], stale.note.unwrap());
        }
        other => panic!("expected conflict with both versions, got {other:?}"),
    }
    let replay = store::create(&pool, &learner, &command).await.unwrap();
    assert_eq!(replay.revision, 1);
    assert_eq!(replay.note.as_deref(), Some(original));
    let history = store::history(&pool, &learner, first.id).await.unwrap();
    assert_eq!(history[1].note.as_deref(), Some(original));
    let recovery = EditMoment {
        expected_revision: revised.revision,
        note: history[1].note.clone(),
        activity_date: history[1].activity_date.clone(),
    };
    assert_eq!(
        store::edit(&pool, &learner, first.id, &recovery)
            .await
            .unwrap()
            .revision,
        3
    );
    assert_eq!(
        store::read(&pool, &learner, first.id)
            .await
            .unwrap()
            .note
            .as_deref(),
        Some(original)
    );

    let photo = include_bytes!("../../../../fixtures/media/practice-photo.jpg");
    let hash = Sha256::digest(photo);
    assert_eq!(
        format!("{hash:x}"),
        "7a7ecb6778ef1b3adfa58fcddd43c62a122380f4042a7e2998c5d8d596d8d934"
    );
    let asset_id: Uuid = sqlx::query_scalar(
        "INSERT INTO assets (household_id, uploader_id, label, storage_key, byte_count, sha256)
         VALUES ($1, $2, 'Fictional practice photo', $3, $4, $5) RETURNING id",
    )
    .bind(household_a)
    .bind(learner.id)
    .bind(format!("fixtures/{household_a}/practice-photo.jpg"))
    .bind(photo.len() as i64)
    .bind(hash.as_slice())
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut photo_only = submission(3, None);
    photo_only.asset_ids = vec![asset_id, Uuid::nil()];
    assert!(matches!(
        store::create(&pool, &learner, &photo_only).await,
        Err(Failure::Missing)
    ));
    photo_only.asset_ids.pop();
    assert!(matches!(
        store::create(&pool, &other, &photo_only).await,
        Err(Failure::Missing)
    ));
    let saved = store::create(&pool, &learner, &photo_only).await.unwrap();
    assert_eq!(saved.note, None);
    assert_eq!(saved.asset_ids, [asset_id]);
    let persisted: (i64, Vec<u8>) = sqlx::query_as(
        "SELECT a.byte_count, a.sha256 FROM assets a JOIN moment_assets l ON l.asset_id = a.id
         WHERE l.moment_id = $1",
    )
    .bind(saved.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(persisted, (photo.len() as i64, hash.to_vec()));
    println!(
        "journal-retry household {household_a}, learner {}, moment {}, attachment {asset_id}",
        learner.id, first.id
    );
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn diary_scope_selects_only_the_current_learner() {
    let url = std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set disposable database URL");
    let pool = storage::connect_lazy(url.parse().unwrap());
    storage::migrate(&pool).await.unwrap();
    let family = household(&pool, "Fictional diary A").await;
    let learner = actor(&pool, family, "owner", "learner").await;
    let sibling = actor(&pool, family, "sibling", "learner").await;
    let guardian = actor(&pool, family, "parent", "guardian").await;
    let guest = actor(&pool, family, "relative", "guest").await;
    let other_family = household(&pool, "Fictional diary B").await;
    let outsider = actor(&pool, other_family, "outsider", "learner").await;
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../fixtures/household.json")).unwrap();
    let private = fixture["moments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["visibility"] == "diary")
        .unwrap();
    let marker = private["title"].as_str().unwrap();
    let ordinary = store::create(&pool, &learner, &submission(41, Some("Family example")))
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    // The operator seeds only invented content. Diary writes have no application route.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO moments
            (household_id, learner_id, recorder_id, submission_key, submission_hash, voice, visibility)
         VALUES ($1, $2, $2, gen_random_uuid(), decode(repeat('00', 32), 'hex'), 'student', 'diary')
         RETURNING id",
    )
    .bind(family)
    .bind(learner.id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note)
         VALUES ($1, $2, 1, $3, $4)",
    )
    .bind(id)
    .bind(family)
    .bind(learner.id)
    .bind(marker)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    for actor in [&learner, &guardian, &sibling] {
        let entries = store::list(&pool, actor).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, ordinary.id);
        assert!(matches!(
            store::read(&pool, actor, id).await,
            Err(Failure::Missing)
        ));
        assert!(matches!(
            store::history(&pool, actor, id).await,
            Err(Failure::Missing)
        ));
    }
    use crate::identity::diary::learner_scope;

    let learner = learner_scope(learner).unwrap();
    assert_eq!(store::diary(&pool, &learner).await.unwrap()[0].id, id);
    assert_eq!(
        store::diary_entry(&pool, &learner, id)
            .await
            .unwrap()
            .note
            .as_deref(),
        Some(marker)
    );
    for actor in [guardian, guest] {
        assert!(matches!(learner_scope(actor), Err(Failure::Missing)));
    }
    for actor in [sibling, outsider] {
        let scope = learner_scope(actor).unwrap();
        assert!(matches!(
            store::diary_entry(&pool, &scope, id).await,
            Err(Failure::Missing)
        ));
        assert!(store::diary(&pool, &scope).await.unwrap().is_empty());
    }
    assert!(matches!(
        store::diary_entry(&pool, &learner, ordinary.id).await,
        Err(Failure::Missing)
    ));
}

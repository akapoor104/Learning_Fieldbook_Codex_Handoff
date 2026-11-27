use super::*;

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL 16 database in FIELDBOOK_TEST_DATABASE_URL"]
async fn attempts_retain_roles_help_and_prior_sources_across_versions() {
    let url = std::env::var("FIELDBOOK_TEST_DATABASE_URL").expect("set disposable database URL");
    let pool = storage::connect_lazy(url.parse().unwrap());
    storage::migrate(&pool).await.unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../../../../fixtures/household.json")).unwrap();
    let home = household(&pool).await;
    let learner = session_named(&pool, home, "learner", "Fictional host").await;
    let guardian = session(&pool, home, "guardian").await;
    let outsider = session(&pool, household(&pool).await, "learner").await;
    let media = TestMedia::new();
    let router = identity::router_with_media(
        pool.clone(),
        ORIGIN,
        "localhost",
        SECRET,
        PathBuf::from("web/dist"),
        Some(PrivateMedia::new(media.0.clone())),
    )
    .unwrap();
    let (address, server) = serve(router).await;

    let domain = uuid(&pool).await;
    let label = format!("/api/v1/household-labels/{domain}");
    assert_eq!(
        call!(
            address,
            &guardian,
            "PUT",
            &label,
            json!({
                "kind": "domain", "name": "Neighborhood seed exchange"
            })
        )
        .0,
        201
    );
    let project = uuid(&pool).await;
    let project_path = format!("/api/v1/projects/{project}");
    let project_input = json!({ "title": "Seed table", "domain_ids": [domain] });
    assert_eq!(
        call!(address, &learner, "PUT", &project_path, project_input).0,
        201
    );
    assert_eq!(call!(address, &guardian, "DELETE", &label).0, 204);
    assert_eq!(
        call!(address, &learner, "PUT", &project_path, project_input).0,
        204
    );
    let other_project = format!("/api/v1/projects/{}", uuid(&pool).await);
    assert_eq!(
        call!(address, &learner, "PUT", &other_project, project_input).0,
        422
    );
    assert_eq!(
        call!(address, &guardian, "POST", &format!("{label}/restore")).0,
        204
    );
    assert_eq!(
        call!(address, &learner, "PUT", &other_project, project_input).0,
        201
    );

    let (status, first) = call!(
        address,
        &learner,
        "POST",
        "/api/v1/moments",
        json!({
            "submission_key": uuid(&pool).await, "note": fixture["moments"][0]["title"],
            "activity_date": fixture["moments"][0]["activity_date"]
        })
    );
    assert_eq!(status, 201);
    let (status, second) = call!(
        address,
        &learner,
        "POST",
        "/api/v1/moments",
        json!({
            "submission_key": uuid(&pool).await, "note": fixture["moments"][1]["title"],
            "activity_date": fixture["moments"][1]["activity_date"]
        })
    );
    assert_eq!(status, 201);
    let first_id = first["id"].as_str().unwrap();
    let second_id = second["id"].as_str().unwrap();
    let first_path = format!("/api/v1/moments/{first_id}/attempt");
    let second_path = format!("/api/v1/moments/{second_id}/attempt");
    let prior = json!({ "project_id": project, "expected_revision": 0, "details": { "event": "First exchange" } });
    assert_eq!(call!(address, &learner, "PUT", &first_path, prior).0, 200);
    let roles: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM household_labels WHERE household_id = $1
         AND kind = 'role' AND name IN ('Participant', 'Host') ORDER BY id",
    )
    .bind(home)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(roles.len(), 2);
    let next = json!({ "project_id": project, "expected_revision": 0, "details": {
        "event": "Second exchange", "my_part": "I counted the seeds.", "help": "A neighbor brought cups.",
        "role_ids": roles, "prior_attempt_id": first_id
    } });
    let (status, saved) = call!(address, &learner, "PUT", &second_path, next);
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        call!(address, &learner, "PUT", &second_path, next),
        (200, saved)
    );
    assert_eq!(call!(address, &outsider, "GET", &second_path).0, 404);
    assert_eq!(call!(address, &guardian, "PUT", &second_path, next).0, 404);

    // The project transaction serializes edits across these two dependent attempts.
    let cycle = json!({ "project_id": project, "expected_revision": 1, "details": { "prior_attempt_id": second_id } });
    assert_eq!(call!(address, &learner, "PUT", &first_path, cycle).0, 422);
    let mut change = next.clone();
    change["expected_revision"] = json!(1);
    change["details"]["help"] = json!("We used the neighbor’s trays.");
    assert_eq!(call!(address, &learner, "PUT", &second_path, change).0, 200);
    let discarded = json!({ "project_id": project, "expected_revision": 1, "details": {} });
    assert_eq!(
        call!(address, &learner, "PUT", &second_path, discarded).0,
        409
    );
    let (status, restored) = call!(
        address,
        &learner,
        "POST",
        &format!("{second_path}/restore"),
        json!({ "expected_revision": 2, "restore_revision": 1 })
    );
    assert_eq!(status, 200, "{restored}");
    assert_eq!(restored["revision"], 3);
    for field in ["event", "my_part", "help", "role_ids", "prior_attempt_id"] {
        assert_eq!(restored["details"][field], next["details"][field]);
    }

    let (attempts, revisions, role_links): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM moment_attempts WHERE household_id = $1),
                (SELECT count(*) FROM attempt_revisions WHERE household_id = $1),
                (SELECT count(*) FROM attempt_roles WHERE household_id = $1)",
    )
    .bind(home)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((attempts, revisions, role_links), (2, 4, 6));
    let (status, timeline) = call!(address, &learner, "GET", &project_path);
    assert_eq!(status, 200, "{timeline}");
    assert_eq!(timeline[0]["moment_id"], first_id);
    assert_eq!(timeline[1]["moment_id"], second_id);
    server.abort();
}

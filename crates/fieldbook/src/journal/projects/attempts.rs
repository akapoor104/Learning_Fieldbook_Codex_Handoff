use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgExecutor, Postgres, Transaction};
use uuid::Uuid;

use super::{begin, check_label, check_project, clean, link_moment, Actor, Auth, Failure};
use crate::{
    identity::routes::family_actor,
    journal::{moment_id, store::lock_moment},
};

pub(super) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/moments/{id}/attempt", get(history).put(save))
        .route("/api/v1/moments/{id}/attempt/restore", post(restore))
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, sqlx::FromRow)]
#[serde(deny_unknown_fields)]
struct Details {
    event: Option<String>,
    event_type_id: Option<Uuid>,
    outcome: Option<String>,
    my_part: Option<String>,
    others_part: Option<String>,
    help: Option<String>,
    checking: Option<String>,
    prior_attempt_id: Option<Uuid>,
    #[serde(default)]
    role_ids: Vec<Uuid>,
}

impl Details {
    fn normalize(&mut self) -> Result<(), Failure> {
        for field in [
            &mut self.event,
            &mut self.outcome,
            &mut self.my_part,
            &mut self.others_part,
            &mut self.help,
            &mut self.checking,
        ] {
            *field = clean(field.as_deref(), 4000)?.map(str::to_owned);
        }
        self.role_ids.sort_unstable();
        self.role_ids.dedup();
        if self.role_ids.len() > 20 {
            return Err(Failure::Validation("too_many_roles"));
        }
        Ok(())
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct Version {
    project_id: Uuid,
    revision: i32,
    editor_id: Uuid,
    editor_name: String,
    #[sqlx(flatten)]
    details: Details,
}

const VERSIONS: &str = "
    SELECT t.project_id, r.revision, r.editor_id, editor.display_name AS editor_name,
           r.event, r.event_type_id, r.outcome, r.my_part, r.others_part, r.help, r.checking, r.prior_attempt_id,
           ARRAY(SELECT label_id FROM attempt_roles WHERE moment_id = r.moment_id AND revision = r.revision
                 ORDER BY label_id) AS role_ids
    FROM moment_attempts t JOIN attempt_revisions r ON r.moment_id = t.moment_id
    JOIN actors editor ON editor.id = r.editor_id
    WHERE t.household_id = $1 AND t.moment_id = $2";

async fn history(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Version>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    let id = moment_id(&id)?;
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM family_moments WHERE id = $1 AND household_id = $2)",
    )
    .bind(id)
    .bind(actor.household_id)
    .fetch_one(&auth.pool)
    .await?;
    if !available {
        return Err(Failure::Missing);
    }
    Ok(Json(
        sqlx::query_as(&format!("{VERSIONS} ORDER BY r.revision DESC"))
            .bind(actor.household_id)
            .bind(id)
            .fetch_all(&auth.pool)
            .await?,
    ))
}

async fn version(
    executor: impl PgExecutor<'_>,
    actor: &Actor,
    id: Uuid,
    revision: Option<i32>,
) -> Result<Option<Version>, Failure> {
    Ok(sqlx::query_as(&format!(
        "{VERSIONS} AND r.revision = coalesce($3, t.revision)"
    ))
    .bind(actor.household_id)
    .bind(id)
    .bind(revision)
    .fetch_optional(executor)
    .await?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveAttempt {
    project_id: Uuid,
    expected_revision: i32,
    details: Details,
}

async fn save(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<SaveAttempt>,
) -> Result<Json<Version>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = moment_id(&id)?;
    let mut tx = begin(&auth.pool, &actor).await?;
    check_writer(&mut tx, &actor, id).await?;
    check_project(&mut *tx, &actor, input.project_id).await?;
    input.details.normalize()?;

    let current = version(&mut *tx, &actor, id, None).await?;
    let saved = save_version(&mut tx, &actor, id, &input, current, false).await?;
    tx.commit().await?;
    Ok(Json(saved))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreAttempt {
    expected_revision: i32,
    restore_revision: i32,
}

async fn restore(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<RestoreAttempt>,
) -> Result<Json<Version>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = moment_id(&id)?;
    let mut tx = begin(&auth.pool, &actor).await?;
    check_writer(&mut tx, &actor, id).await?;
    let restored = version(&mut *tx, &actor, id, Some(input.restore_revision))
        .await?
        .ok_or(Failure::Missing)?;
    let current = version(&mut *tx, &actor, id, None).await?;
    let saved = save_version(
        &mut tx,
        &actor,
        id,
        &SaveAttempt {
            project_id: restored.project_id,
            expected_revision: input.expected_revision,
            details: restored.details,
        },
        current,
        true,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(saved))
}

async fn check_writer(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
) -> Result<(), Failure> {
    if lock_moment(tx, actor.household_id, id).await?.writer() != actor.id {
        return Err(Failure::Missing);
    }
    Ok(())
}

async fn save_version(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    input: &SaveAttempt,
    current: Option<Version>,
    restoring: bool,
) -> Result<Version, Failure> {
    let expected = input.expected_revision;
    let details = &input.details;
    if let Some(current) = current.as_ref() {
        if current.project_id != input.project_id {
            return Err(Failure::Conflict);
        }
        if current.details == *details
            && (current.revision == expected || Some(current.revision) == expected.checked_add(1))
        {
            return version(&mut **tx, actor, id, None)
                .await?
                .ok_or(Failure::Missing);
        }
    }
    let revision = current.as_ref().map_or(0, |item| item.revision);
    if revision != expected {
        return Err(Failure::Conflict);
    }
    let next = revision.checked_add(1).ok_or(Failure::Conflict)?;
    let prior_details = current.as_ref().map(|item| &item.details);
    if let Some(event_type_id) = details.event_type_id {
        let keep =
            restoring || prior_details.is_some_and(|old| old.event_type_id == Some(event_type_id));
        check_label(tx, actor, event_type_id, "event_type", keep).await?;
    }
    for role in &details.role_ids {
        let keep = restoring || prior_details.is_some_and(|old| old.role_ids.contains(role));
        check_label(tx, actor, *role, "role", keep).await?;
    }
    if let Some(prior) = details.prior_attempt_id {
        check_prior(tx, actor, input.project_id, id, prior).await?;
    }
    if current.is_none() {
        link_moment(tx, actor, input.project_id, id).await?;
    }
    sqlx::query(
        "INSERT INTO moment_attempts (moment_id, household_id, project_id, revision) VALUES ($1, $2, $3, $4)
         ON CONFLICT (moment_id) DO UPDATE SET revision = excluded.revision",
    ).bind(id).bind(actor.household_id).bind(input.project_id).bind(next).execute(&mut **tx).await?;
    sqlx::query(
        "INSERT INTO attempt_revisions
         (moment_id, household_id, revision, editor_id, event, event_type_id, outcome, my_part, others_part, help, checking, prior_attempt_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    ).bind(id).bind(actor.household_id).bind(next).bind(actor.id)
        .bind(&details.event).bind(details.event_type_id).bind(&details.outcome).bind(&details.my_part)
        .bind(&details.others_part).bind(&details.help).bind(&details.checking).bind(details.prior_attempt_id)
        .execute(&mut **tx).await?;
    sqlx::query("INSERT INTO attempt_roles (moment_id, household_id, revision, label_id) SELECT $1, $2, $3, unnest($4::uuid[])")
        .bind(id).bind(actor.household_id).bind(next).bind(&details.role_ids).execute(&mut **tx).await?;
    version(&mut **tx, actor, id, Some(next))
        .await?
        .ok_or(Failure::Missing)
}

async fn check_prior(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    project: Uuid,
    id: Uuid,
    prior: Uuid,
) -> Result<(), Failure> {
    if id == prior {
        return Err(Failure::Validation("attempt_cycle"));
    }
    let (found, cyclic): (bool, bool) = sqlx::query_as(
        "WITH RECURSIVE chain(moment_id, prior_attempt_id) AS (
             SELECT a.moment_id, r.prior_attempt_id FROM moment_attempts a
             JOIN attempt_revisions r ON r.moment_id = a.moment_id AND r.revision = a.revision
             WHERE a.moment_id = $1 AND a.household_id = $2 AND a.project_id = $3
             UNION
             SELECT a.moment_id, r.prior_attempt_id FROM chain c JOIN moment_attempts a ON a.moment_id = c.prior_attempt_id
             JOIN attempt_revisions r ON r.moment_id = a.moment_id AND r.revision = a.revision
             WHERE a.household_id = $2 AND a.project_id = $3
         )
         SELECT EXISTS (SELECT 1 FROM chain), EXISTS (SELECT 1 FROM chain WHERE moment_id = $4)",
    ).bind(prior).bind(actor.household_id).bind(project).bind(id).fetch_one(&mut **tx).await?;
    if !found {
        return Err(Failure::Missing);
    }
    if cyclic {
        return Err(Failure::Validation("attempt_cycle"));
    }
    Ok(())
}

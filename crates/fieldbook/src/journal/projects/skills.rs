use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{begin, check_label, created, required, Actor, Auth, Failure};
use crate::{
    identity::routes::family_actor,
    journal::{moment_id, store::lock_moment},
};

pub(super) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/skills/{id}", get(read))
        .route(
            "/api/v1/moments/{id}/skills/{skill}",
            put(save_evidence).delete(withdraw_evidence),
        )
        .route(
            "/api/v1/moments/{id}/skills/{skill}/restore",
            post(restore_evidence),
        )
        .route(
            "/api/v1/skill-transfers/{id}",
            put(save_transfer).delete(withdraw_transfer),
        )
        .route(
            "/api/v1/skill-transfers/{id}/restore",
            post(restore_transfer),
        )
}

async fn learner(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    moment: Uuid,
) -> Result<(), Failure> {
    if actor.role != "learner"
        || lock_moment(tx, actor.household_id, moment)
            .await?
            .learner_id
            != actor.id
    {
        return Err(Failure::Missing);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveEvidence {
    reason: String,
    independence: Option<String>,
}

async fn save_evidence(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, skill)): Path<(String, String)>,
    Json(input): Json<SaveEvidence>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let (id, skill) = (moment_id(&id)?, moment_id(&skill)?);
    let mut tx = begin(&auth.pool, &actor).await?;
    learner(&mut tx, &actor, id).await?;
    let reason = required(&input.reason, 4000)?;
    if input
        .independence
        .as_deref()
        .is_some_and(|value| !matches!(value, "with_help" | "on_my_own"))
    {
        return Err(Failure::Validation("invalid_independence"));
    }
    let existing: Option<(String, Option<String>, bool)> = sqlx::query_as(
        "SELECT reason, independence, withdrawn_at IS NOT NULL FROM moment_skills
         WHERE moment_id = $1 AND label_id = $2 AND household_id = $3 AND learner_id = $4",
    )
    .bind(id)
    .bind(skill)
    .bind(actor.household_id)
    .bind(actor.id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((prior, independence, withdrawn)) = existing {
        if withdrawn || prior != reason || independence != input.independence {
            return Err(Failure::Conflict);
        }
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    check_label(&mut tx, &actor, skill, "skill", false).await?;
    sqlx::query(
        "INSERT INTO moment_skills (moment_id, label_id, household_id, learner_id, reason, independence)
         VALUES ($1, $2, $3, $4, $5, $6)",
    ).bind(id).bind(skill).bind(actor.household_id).bind(actor.id).bind(reason).bind(input.independence)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(created(
        format!("/api/v1/moments/{id}/skills/{skill}"),
        skill,
    ))
}

async fn withdraw_evidence(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, skill)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    change_evidence(&auth, &headers, moment_id(&id)?, moment_id(&skill)?, true).await
}

async fn restore_evidence(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, skill)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    change_evidence(&auth, &headers, moment_id(&id)?, moment_id(&skill)?, false).await
}

async fn change_evidence(
    auth: &Auth,
    headers: &HeaderMap,
    id: Uuid,
    skill: Uuid,
    withdraw: bool,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    let mut tx = begin(&auth.pool, &actor).await?;
    learner(&mut tx, &actor, id).await?;
    let result = sqlx::query(
        "UPDATE moment_skills SET withdrawn_at = CASE WHEN $5 THEN coalesce(withdrawn_at, now()) ELSE NULL END
         WHERE household_id = $1 AND moment_id = $2 AND label_id = $3 AND learner_id = $4",
    ).bind(actor.household_id).bind(id).bind(skill).bind(actor.id).bind(withdraw).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, PartialEq, sqlx::FromRow)]
#[serde(deny_unknown_fields)]
struct SaveTransfer {
    label_id: Uuid,
    from_project_id: Uuid,
    from_moment_id: Uuid,
    to_project_id: Uuid,
    to_moment_id: Uuid,
    reason: String,
}

async fn endpoints(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    item: &SaveTransfer,
) -> Result<(), Failure> {
    if item.from_project_id == item.to_project_id || item.from_moment_id == item.to_moment_id {
        return Err(Failure::Validation("different_transfer_sources_required"));
    }
    for (project, moment) in [
        (item.from_project_id, item.from_moment_id),
        (item.to_project_id, item.to_moment_id),
    ] {
        let found: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM project_moments p
             JOIN family_moments m ON m.id = p.moment_id AND m.household_id = p.household_id
             JOIN moment_skills s ON s.moment_id = m.id AND s.household_id = m.household_id
             WHERE p.household_id = $1 AND p.project_id = $2 AND m.id = $3 AND m.learner_id = $4
               AND s.label_id = $5 AND s.learner_id = $4 AND p.withdrawn_at IS NULL AND s.withdrawn_at IS NULL)",
        ).bind(actor.household_id).bind(project).bind(moment).bind(actor.id).bind(item.label_id)
            .fetch_one(&mut **tx).await?;
        if !found {
            return Err(Failure::Missing);
        }
    }
    Ok(())
}

async fn save_transfer(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<SaveTransfer>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    if actor.role != "learner" {
        return Err(Failure::Missing);
    }
    let id = moment_id(&id)?;
    input.reason = required(&input.reason, 4000)?.to_owned();
    let mut tx = begin(&auth.pool, &actor).await?;
    endpoints(&mut tx, &actor, &input).await?;
    let saved = sqlx::query_as::<_, SaveTransfer>(
        "SELECT label_id, from_project_id, from_moment_id, to_project_id, to_moment_id, reason
         FROM skill_transfers WHERE id = $1 AND household_id = $2 AND learner_id = $3 AND withdrawn_at IS NULL",
    ).bind(id).bind(actor.household_id).bind(actor.id).fetch_optional(&mut *tx).await?;
    if let Some(saved) = saved {
        if saved != input {
            return Err(Failure::Conflict);
        }
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    check_label(&mut tx, &actor, input.label_id, "skill", false).await?;
    let inserted = sqlx::query(
        "INSERT INTO skill_transfers (id, household_id, learner_id, label_id, from_project_id, from_moment_id,
                                      to_project_id, to_moment_id, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT DO NOTHING",
    ).bind(id).bind(actor.household_id).bind(actor.id).bind(input.label_id)
        .bind(input.from_project_id).bind(input.from_moment_id).bind(input.to_project_id).bind(input.to_moment_id)
        .bind(&input.reason).execute(&mut *tx).await?;
    if inserted.rows_affected() == 0 {
        return Err(Failure::Conflict);
    }
    tx.commit().await?;
    Ok(created(format!("/api/v1/skill-transfers/{id}"), id))
}

async fn withdraw_transfer(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Failure> {
    change_transfer(&auth, &headers, moment_id(&id)?, true).await
}

async fn restore_transfer(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Failure> {
    change_transfer(&auth, &headers, moment_id(&id)?, false).await
}

async fn change_transfer(
    auth: &Auth,
    headers: &HeaderMap,
    id: Uuid,
    withdraw: bool,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    if actor.role != "learner" {
        return Err(Failure::Missing);
    }
    let mut tx = begin(&auth.pool, &actor).await?;
    if !withdraw {
        let saved = sqlx::query_as(
            "SELECT label_id, from_project_id, from_moment_id, to_project_id, to_moment_id, reason
             FROM skill_transfers WHERE id = $1 AND household_id = $2 AND learner_id = $3",
        )
        .bind(id)
        .bind(actor.household_id)
        .bind(actor.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Failure::Missing)?;
        endpoints(&mut tx, &actor, &saved).await?;
    }
    let result = sqlx::query(
        "UPDATE skill_transfers SET withdrawn_at = CASE WHEN $4 THEN coalesce(withdrawn_at, now()) ELSE NULL END
         WHERE id = $1 AND household_id = $2 AND learner_id = $3",
    ).bind(id).bind(actor.household_id).bind(actor.id).bind(withdraw).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
struct Evidence {
    moment_id: Uuid,
    note: Option<String>,
    activity_date: Option<String>,
    learner_id: Uuid,
    learner_name: String,
    recorder_name: String,
    voice: String,
    reason: String,
    independence: Option<String>,
    withdrawn: bool,
    project_ids: Vec<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
struct Transfer {
    id: Uuid,
    learner_id: Uuid,
    learner_name: String,
    from_project_id: Uuid,
    from_moment_id: Uuid,
    to_project_id: Uuid,
    to_moment_id: Uuid,
    reason: String,
    withdrawn: bool,
    sources_active: bool,
}

#[derive(Serialize)]
struct Skill {
    evidence: Vec<Evidence>,
    transfers: Vec<Transfer>,
}

async fn read(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Skill>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    let id = moment_id(&id)?;
    let mut tx = auth.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    check_label(&mut tx, &actor, id, "skill", true).await?;
    let evidence = sqlx::query_as(
        "SELECT s.moment_id, r.note, to_char(r.activity_date, 'YYYY-MM-DD') AS activity_date,
                s.learner_id, learner.display_name AS learner_name, recorder.display_name AS recorder_name,
                m.voice, s.reason, s.independence, s.withdrawn_at IS NOT NULL AS withdrawn,
                ARRAY(SELECT project_id FROM project_moments WHERE moment_id = m.id AND withdrawn_at IS NULL
                      ORDER BY project_id) AS project_ids
         FROM moment_skills s JOIN family_moments m ON m.id = s.moment_id AND m.household_id = s.household_id
         JOIN moment_revisions r ON r.moment_id = m.id AND r.revision = m.revision
         JOIN actors learner ON learner.id = s.learner_id
         JOIN actors recorder ON recorder.id = m.recorder_id
         WHERE s.household_id = $1 AND s.label_id = $2 AND (s.withdrawn_at IS NULL OR s.learner_id = $3)
         ORDER BY coalesce(r.activity_date, (m.created_at AT TIME ZONE 'America/Los_Angeles')::date), m.created_at, m.id",
    ).bind(actor.household_id).bind(id).bind(actor.id).fetch_all(&mut *tx).await?;
    let transfers = sqlx::query_as(
        "SELECT t.id, t.learner_id, a.display_name AS learner_name, t.from_project_id, t.from_moment_id,
                t.to_project_id, t.to_moment_id, t.reason, t.withdrawn_at IS NOT NULL AS withdrawn, scope.sources_active
         FROM skill_transfers t JOIN actors a ON a.id = t.learner_id
         JOIN project_moments fp ON fp.project_id = t.from_project_id AND fp.moment_id = t.from_moment_id
         JOIN project_moments tp ON tp.project_id = t.to_project_id AND tp.moment_id = t.to_moment_id
         JOIN moment_skills fs ON fs.label_id = t.label_id AND fs.moment_id = t.from_moment_id
         JOIN moment_skills ts ON ts.label_id = t.label_id AND ts.moment_id = t.to_moment_id
         CROSS JOIN LATERAL (SELECT fp.withdrawn_at IS NULL AND tp.withdrawn_at IS NULL AND
                      fs.withdrawn_at IS NULL AND ts.withdrawn_at IS NULL AS sources_active) scope
         WHERE t.household_id = $1 AND t.label_id = $2
           AND ((t.withdrawn_at IS NULL AND scope.sources_active) OR t.learner_id = $3)
         ORDER BY t.id",
    ).bind(actor.household_id).bind(id).bind(actor.id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(Skill {
        evidence,
        transfers,
    }))
}

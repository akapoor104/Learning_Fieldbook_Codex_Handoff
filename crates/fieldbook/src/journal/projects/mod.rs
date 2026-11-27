use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::{moment_id, Actor, Auth, Failure};
use crate::identity::routes::family_actor;

mod attempts;
mod skills;

pub(super) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/projects", get(catalog))
        .route("/api/v1/projects/{id}", get(timeline).put(save_project))
        .route(
            "/api/v1/projects/{id}/moments/{moment}",
            put(link).delete(unlink),
        )
        .route(
            "/api/v1/household-labels/{id}",
            put(save_label).delete(retire_label),
        )
        .route(
            "/api/v1/household-labels/{id}/restore",
            axum::routing::post(restore_label),
        )
        .merge(attempts::router())
        .merge(skills::router())
}

fn created(path: String, id: Uuid) -> Response {
    (
        StatusCode::CREATED,
        [(header::LOCATION, path)],
        Json(serde_json::json!({ "id": id })),
    )
        .into_response()
}

#[derive(Serialize, sqlx::FromRow)]
struct Label {
    id: Uuid,
    kind: String,
    name: String,
    retired: bool,
}

#[derive(Serialize, sqlx::FromRow)]
struct Project {
    id: Uuid,
    title: String,
    type_id: Option<Uuid>,
    domain_ids: Vec<Uuid>,
    question: Option<String>,
    success_criterion: Option<String>,
    state: String,
}

#[derive(Serialize)]
struct Catalog {
    projects: Vec<Project>,
    labels: Vec<Label>,
}

async fn catalog(State(auth): State<Auth>, headers: HeaderMap) -> Result<Json<Catalog>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    let projects = sqlx::query_as(
        "SELECT p.id, p.title, p.type_id, p.question, p.success_criterion, p.state,
                ARRAY(SELECT label_id FROM project_domains WHERE project_id = p.id ORDER BY label_id) AS domain_ids
         FROM projects p WHERE p.household_id = $1 ORDER BY lower(p.title), p.id",
    )
    .bind(actor.household_id)
    .fetch_all(&auth.pool)
    .await?;
    let labels = sqlx::query_as(
        "SELECT id, kind, name, retired_at IS NOT NULL AS retired FROM household_labels
         WHERE household_id = $1 ORDER BY kind, lower(name), id",
    )
    .bind(actor.household_id)
    .fetch_all(&auth.pool)
    .await?;
    Ok(Json(Catalog { projects, labels }))
}

// These short transactions serialize household label selection, chronology edits,
// and evidence changes that touch more than one relationship.
async fn begin(pool: &PgPool, actor: &Actor) -> Result<Transaction<'static, Postgres>, Failure> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM households WHERE id = $1 FOR NO KEY UPDATE")
        .bind(actor.household_id)
        .fetch_one(&mut *tx)
        .await?;
    Ok(tx)
}

fn clean(value: Option<&str>, max: usize) -> Result<Option<&str>, Failure> {
    let cleaned = value.map(str::trim).filter(|text| !text.is_empty());
    if cleaned.is_some_and(|text| text.len() > max || text.contains('\0')) {
        return Err(Failure::Validation("invalid_project_text"));
    }
    Ok(cleaned)
}

fn required(value: &str, max: usize) -> Result<&str, Failure> {
    clean(Some(value), max)?.ok_or(Failure::Validation("project_text_required"))
}

async fn check_label(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    kind: &str,
    keep: bool,
) -> Result<(), Failure> {
    let retired: bool = sqlx::query_scalar(
        "SELECT retired_at IS NOT NULL FROM household_labels WHERE household_id = $1 AND id = $2 AND kind = $3",
    ).bind(actor.household_id).bind(id).bind(kind).fetch_optional(&mut **tx).await?
        .ok_or(Failure::Missing)?;
    if retired && !keep {
        return Err(Failure::Validation("retired_label"));
    }
    Ok(())
}

async fn label_owner(auth: &Auth, headers: &HeaderMap) -> Result<Actor, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    if actor.role != "guardian" {
        return Err(Failure::Missing);
    }
    Ok(actor)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveLabel {
    name: String,
    kind: String,
    expected_name: Option<String>,
}

async fn save_label(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SaveLabel>,
) -> Result<Response, Failure> {
    let actor = label_owner(&auth, &headers).await?;
    let id = moment_id(&id)?;
    let name = required(&input.name, 200)?;
    if !matches!(
        input.kind.as_str(),
        "domain" | "project_type" | "event_type" | "role" | "skill" | "tag"
    ) {
        return Err(Failure::Validation("invalid_label_kind"));
    }
    let mut tx = begin(&auth.pool, &actor).await?;
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT kind, name FROM household_labels WHERE household_id = $1 AND id = $2",
    )
    .bind(actor.household_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((kind, current)) = &existing {
        if kind != &input.kind || (current != name && input.expected_name.as_ref() != Some(current))
        {
            return Err(Failure::Conflict);
        }
    } else if input.expected_name.is_some() {
        return Err(Failure::Missing);
    }
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM household_labels WHERE household_id = $1 AND id <> $2
                        AND kind = $3 AND lower(name) = lower($4))",
    )
    .bind(actor.household_id)
    .bind(id)
    .bind(&input.kind)
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    if duplicate {
        return Err(Failure::Conflict);
    }
    let updated = sqlx::query(
        "INSERT INTO household_labels (id, household_id, kind, name) VALUES ($1, $2, $3, $4)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name
         WHERE household_labels.household_id = excluded.household_id",
    )
    .bind(id)
    .bind(actor.household_id)
    .bind(&input.kind)
    .bind(name)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(if existing.is_some() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        created(format!("/api/v1/household-labels/{id}"), id)
    })
}

async fn retire_label(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Failure> {
    change_label(&auth, &headers, &id, true).await
}

async fn restore_label(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, Failure> {
    change_label(&auth, &headers, &id, false).await
}

async fn change_label(
    auth: &Auth,
    headers: &HeaderMap,
    id: &str,
    retire: bool,
) -> Result<StatusCode, Failure> {
    let actor = label_owner(auth, headers).await?;
    let mut tx = begin(&auth.pool, &actor).await?;
    let result = sqlx::query(
        "UPDATE household_labels SET retired_at = CASE WHEN $3 THEN coalesce(retired_at, now()) ELSE NULL END
         WHERE id = $1 AND household_id = $2",
    ).bind(moment_id(id)?).bind(actor.household_id).bind(retire).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveProject {
    title: String,
    type_id: Option<Uuid>,
    #[serde(default)]
    domain_ids: Vec<Uuid>,
    question: Option<String>,
    success_criterion: Option<String>,
}

async fn save_project(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut input): Json<SaveProject>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = moment_id(&id)?;
    let title = required(&input.title, 200)?;
    let question = clean(input.question.as_deref(), 4000)?;
    let criterion = clean(input.success_criterion.as_deref(), 4000)?;
    input.domain_ids.sort_unstable();
    input.domain_ids.dedup();
    if input.domain_ids.len() > 20 {
        return Err(Failure::Validation("too_many_domains"));
    }
    let mut tx = begin(&auth.pool, &actor).await?;
    let saved: Option<Project> = sqlx::query_as(
        "SELECT p.id, p.title, p.type_id, p.question, p.success_criterion, p.state,
                ARRAY(SELECT label_id FROM project_domains WHERE project_id = p.id ORDER BY label_id) AS domain_ids
         FROM projects p WHERE p.id = $1 AND p.household_id = $2 AND p.created_by = $3",
    ).bind(id).bind(actor.household_id).bind(actor.id).fetch_optional(&mut *tx).await?;
    if let Some(saved) = saved {
        if saved.title != title
            || saved.type_id != input.type_id
            || saved.domain_ids != input.domain_ids
            || saved.question.as_deref() != question
            || saved.success_criterion.as_deref() != criterion
        {
            return Err(Failure::Conflict);
        }
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    if let Some(type_id) = input.type_id {
        check_label(&mut tx, &actor, type_id, "project_type", false).await?;
    }
    for label in &input.domain_ids {
        check_label(&mut tx, &actor, *label, "domain", false).await?;
    }
    let result = sqlx::query(
        "INSERT INTO projects (id, household_id, created_by, title, type_id, question, success_criterion)
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
    ).bind(id).bind(actor.household_id).bind(actor.id).bind(title).bind(input.type_id)
        .bind(question).bind(criterion).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    sqlx::query("INSERT INTO project_domains (project_id, household_id, label_id) SELECT $1, $2, unnest($3::uuid[])")
        .bind(id).bind(actor.household_id).bind(&input.domain_ids).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(created(format!("/api/v1/projects/{id}"), id))
}

async fn check_project(
    executor: impl sqlx::PgExecutor<'_>,
    actor: &Actor,
    project: Uuid,
) -> Result<(), Failure> {
    let found = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects WHERE id = $1 AND household_id = $2",
    )
    .bind(project)
    .bind(actor.household_id)
    .fetch_optional(executor)
    .await?;
    found.map(|_| ()).ok_or(Failure::Missing)
}

async fn link(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((project, moment)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let mut tx = begin(&auth.pool, &actor).await?;
    let (project, moment) = (moment_id(&project)?, moment_id(&moment)?);
    check_project(&mut *tx, &actor, project).await?;
    super::store::lock_moment(&mut tx, actor.household_id, moment).await?;
    link_moment(&mut tx, &actor, project, moment).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn link_moment(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    project: Uuid,
    moment: Uuid,
) -> Result<(), Failure> {
    sqlx::query(
        "INSERT INTO project_moments (project_id, moment_id, household_id, linked_by)
         VALUES ($1, $2, $3, $4) ON CONFLICT (project_id, moment_id)
         DO UPDATE SET withdrawn_at = NULL WHERE project_moments.withdrawn_at IS NOT NULL",
    )
    .bind(project)
    .bind(moment)
    .bind(actor.household_id)
    .bind(actor.id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn unlink(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((project, moment)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let mut tx = begin(&auth.pool, &actor).await?;
    let (project, moment) = (moment_id(&project)?, moment_id(&moment)?);
    let primary: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM moment_attempts WHERE project_id = $1 AND moment_id = $2 AND household_id = $3)",
    ).bind(project).bind(moment).bind(actor.household_id).fetch_one(&mut *tx).await?;
    if primary {
        return Err(Failure::Conflict);
    }
    let result = sqlx::query(
        "UPDATE project_moments SET withdrawn_at = coalesce(withdrawn_at, now())
         WHERE project_id = $1 AND moment_id = $2 AND household_id = $3",
    )
    .bind(project)
    .bind(moment)
    .bind(actor.household_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
struct TimelineEntry {
    moment_id: Uuid,
    note: Option<String>,
    activity_date: Option<String>,
    occurred_on: String,
    recorded_at: String,
    recorder_name: String,
    learner_name: String,
    voice: String,
    primary_project_id: Option<Uuid>,
    withdrawn: bool,
}

async fn timeline(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<TimelineEntry>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    let id = moment_id(&id)?;
    check_project(&auth.pool, &actor, id).await?;
    Ok(Json(sqlx::query_as(
        r#"SELECT m.id AS moment_id, r.note, to_char(r.activity_date, 'YYYY-MM-DD') AS activity_date,
                to_char(coalesce(r.activity_date, (m.created_at AT TIME ZONE 'America/Los_Angeles')::date), 'YYYY-MM-DD') AS occurred_on,
                to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS recorded_at,
                recorder.display_name AS recorder_name, learner.display_name AS learner_name, m.voice,
                t.project_id AS primary_project_id, l.withdrawn_at IS NOT NULL AS withdrawn
         FROM project_moments l JOIN family_moments m ON m.id = l.moment_id AND m.household_id = l.household_id
         JOIN moment_revisions r ON r.moment_id = m.id AND r.revision = m.revision
         JOIN actors recorder ON recorder.id = m.recorder_id
         JOIN actors learner ON learner.id = m.learner_id
         LEFT JOIN moment_attempts t ON t.moment_id = m.id
         WHERE l.project_id = $1 AND l.household_id = $2
         ORDER BY coalesce(r.activity_date, (m.created_at AT TIME ZONE 'America/Los_Angeles')::date), m.created_at, m.id"#,
    ).bind(id).bind(actor.household_id).fetch_all(&auth.pool).await?))
}

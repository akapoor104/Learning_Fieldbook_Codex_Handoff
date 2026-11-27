use axum::{extract::State, http::HeaderMap, response::Response, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;
use webauthn_rs::prelude::PublicKeyCredential;

use super::{
    routes::{active_session, finish_assertion, start_assertion, Finish},
    store::{self, Purpose},
    Actor, Auth, Failure, CSRF,
};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    View,
    Recovery,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccessRequest {
    learner_id: Uuid,
    kind: Kind,
}

pub(super) async fn start(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(request): Json<AccessRequest>,
) -> Result<Json<Value>, Failure> {
    let (hash, guardian) = guardian_session(&auth, &headers).await?;
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM actors WHERE id = $1 AND household_id = $2 AND role = 'learner'
        )",
    )
    .bind(request.learner_id)
    .bind(guardian.household_id)
    .fetch_one(&auth.pool)
    .await?;
    if !available || request.learner_id.is_nil() {
        return Err(Failure::Missing);
    }
    let passkey = store::passkey(&auth.pool, guardian.id).await?;
    start_assertion(
        &auth,
        &guardian,
        passkey,
        Purpose::Diary,
        Some(&hash),
        Some(request),
    )
    .await
}

pub(super) async fn finish(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<Finish<PublicKeyCredential>>,
) -> Result<Response, Failure> {
    let (hash, _) = guardian_session(&auth, &headers).await?;
    finish_assertion(&auth, input, Purpose::Diary, Some(&hash)).await
}

async fn guardian_session(auth: &Auth, headers: &HeaderMap) -> Result<(Vec<u8>, Actor), Failure> {
    let (_, hash, session) = active_session(auth, headers, true).await?;
    if session.actor.role != "guardian" || session.diary_learner_id.is_some() {
        return Err(Failure::Forbidden);
    }
    Ok((hash, session.actor))
}

pub(super) async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    guardian: &Actor,
    request: AccessRequest,
    session_expiry: OffsetDateTime,
) -> Result<Uuid, Failure> {
    let kind = match request.kind {
        Kind::View => "view",
        Kind::Recovery => "recovery",
    };
    sqlx::query_scalar(
        "WITH learner AS (
             SELECT id FROM actors WHERE id = $1 AND household_id = $2 AND role = 'learner' FOR SHARE
         ), started AS (
             SELECT statement_timestamp() AS at
         )
         INSERT INTO diary_access_events (household_id, learner_id, guardian_id, kind, recorded_at, expires_at)
         SELECT $2, learner.id, $3, $4, started.at, least($5, started.at + interval '5 minutes')
         FROM learner CROSS JOIN started WHERE $5 > started.at
         RETURNING id",
    )
    .bind(request.learner_id)
    .bind(guardian.household_id)
    .bind(guardian.id)
    .bind(kind)
    .bind(session_expiry)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(Failure::Missing)
}

pub(crate) struct ReadScope {
    pub actor: Actor,
    pub learner_id: Uuid,
    guardian_session_hash: Option<Vec<u8>>,
}

impl ReadScope {
    pub fn guardian_session_hash(&self) -> Option<&[u8]> {
        self.guardian_session_hash.as_deref()
    }
}

fn learner_id(actor: &Actor) -> Result<Uuid, Failure> {
    if actor.role == "learner" && !actor.id.is_nil() {
        Ok(actor.id)
    } else {
        Err(Failure::Missing)
    }
}

pub(crate) fn learner_scope(actor: Actor) -> Result<ReadScope, Failure> {
    let learner_id = learner_id(&actor)?;
    Ok(ReadScope {
        actor,
        learner_id,
        guardian_session_hash: None,
    })
}

pub(crate) async fn read_scope(auth: &Auth, headers: &HeaderMap) -> Result<ReadScope, Failure> {
    let (_, hash, session) = active_session(auth, headers, headers.contains_key(CSRF)).await?;
    if session.actor.role != "guardian" {
        return learner_scope(session.actor);
    }
    Ok(ReadScope {
        actor: session.actor,
        learner_id: session.diary_learner_id.ok_or(Failure::Missing)?,
        guardian_session_hash: Some(hash),
    })
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct AccessEvent {
    id: Uuid,
    guardian_id: Uuid,
    guardian_name: String,
    kind: String,
    recorded_at: String,
    expires_at: String,
}

async fn events(pool: &PgPool, learner: &Actor) -> Result<Vec<AccessEvent>, Failure> {
    let learner_id = learner_id(learner)?;
    Ok(sqlx::query_as(
        "SELECT d.id, d.guardian_id, a.display_name AS guardian_name, d.kind,
            to_char(d.recorded_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS recorded_at,
            to_char(d.expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS expires_at
         FROM diary_access_events d
         JOIN actors a ON a.id = d.guardian_id AND a.household_id = d.household_id
         WHERE d.household_id = $1 AND d.learner_id = $2
         ORDER BY d.recorded_at DESC, d.id DESC",
    )
    .bind(learner.household_id)
    .bind(learner_id)
    .fetch_all(pool)
    .await?)
}

pub(super) async fn history(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Vec<AccessEvent>>, Failure> {
    let (_, _, session) = active_session(&auth, &headers, headers.contains_key(CSRF)).await?;
    Ok(Json(events(&auth.pool, &session.actor).await?))
}

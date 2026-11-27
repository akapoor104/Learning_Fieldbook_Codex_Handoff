use crate::journal::{
    add_scoped_comment, scoped_comments, set_scoped_withdrawal, AddComment, Comment,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json as DbJson, Postgres, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::{
    routes::{active_session, family_actor},
    Actor, Auth, Failure, CSRF,
};

pub(super) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/shares", get(guest_list))
        .route("/api/v1/moments/{id}/shares", get(family_review))
        .route(
            "/api/v1/shares/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/api/v1/shares/{id}/comments/{comment}",
            delete(withdraw_comment),
        )
        .route(
            "/api/v1/shares/{id}/comments/{comment}/restore",
            post(restore_comment),
        )
        .route("/api/v1/shares/{id}", get(read).put(approve).delete(revoke))
        .route("/api/v1/shares/{id}/accept", post(accept))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    guest_id: Uuid,
    moment_id: Uuid,
    revision: i32,
    include_text: bool,
    original_asset_ids: Vec<Uuid>,
}

#[derive(Serialize)]
struct Invitation {
    id: Uuid,
    token: String,
}

#[derive(sqlx::FromRow)]
struct PendingSelection {
    guest_id: Uuid,
    moment_id: Uuid,
    revision: i32,
    include_text: bool,
    approved_by: Uuid,
    original_asset_ids: Vec<Uuid>,
    pending: bool,
}

async fn approve(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut selection): Json<Selection>,
) -> Result<Response, Failure> {
    let guardian = guardian_actor(&auth, &headers).await?;
    let id = share_id(&id)?;
    selection.original_asset_ids.sort_unstable();
    if selection.original_asset_ids.len() > 20
        || selection
            .original_asset_ids
            .windows(2)
            .any(|ids| ids[0] == ids[1])
        || (!selection.include_text && selection.original_asset_ids.is_empty())
    {
        return Err(Failure::Validation("invalid_share_selection"));
    }
    let mut tx = auth.pool.begin().await?;
    // Edits, confirmations and attachment changes take the same moment lock.
    let (current_revision, voice): (i32, String) = sqlx::query_as(
        "SELECT revision, voice FROM family_moments
         WHERE id = $1 AND household_id = $2 FOR NO KEY UPDATE",
    )
    .bind(selection.moment_id)
    .bind(guardian.household_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Missing)?;
    let previous: Option<PendingSelection> = sqlx::query_as(
        "SELECT s.guest_id, s.moment_id, s.revision, s.include_text, s.approved_by,
                ARRAY(SELECT asset_id FROM relative_share_originals WHERE share_id = s.id AND household_id = s.household_id
                      ORDER BY asset_id) AS original_asset_ids,
                (s.accepted_at IS NULL AND s.revoked_at IS NULL
                 AND s.invitation_expires_at > statement_timestamp()) AS pending
         FROM relative_shares s WHERE s.id = $1 AND s.household_id = $2
         FOR UPDATE OF s",
    )
    .bind(id)
    .bind(guardian.household_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(previous) = previous {
        if !previous.pending
            || previous.approved_by != guardian.id
            || previous.guest_id != selection.guest_id
            || previous.moment_id != selection.moment_id
            || previous.revision != selection.revision
            || previous.include_text != selection.include_text
            || previous.original_asset_ids != selection.original_asset_ids
        {
            return Err(Failure::Conflict);
        }
        tx.commit().await?;
        return Ok(invitation(&auth, guardian.household_id, id, StatusCode::OK));
    }
    if selection.revision != current_revision {
        return Err(Failure::Conflict);
    }
    if selection.include_text {
        let (has_note, confirmed): (bool, bool) = sqlx::query_as(
            "SELECT r.note IS NOT NULL, coalesce(c.withdrawn_at IS NULL AND c.confirmed_at IS NOT NULL, false)
             FROM moment_revisions r LEFT JOIN moment_confirmations c
               ON c.moment_id = r.moment_id AND c.revision = r.revision
             WHERE r.moment_id = $1 AND r.revision = $2",
        )
        .bind(selection.moment_id)
        .bind(selection.revision)
        .fetch_one(&mut *tx)
        .await?;
        if !has_note || (voice == "scribed" && !confirmed) {
            return Err(Failure::Validation("share_text_unavailable"));
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO relative_shares (id, household_id, moment_id, revision, include_text, guest_id, approved_by)
         SELECT $1, household_id, $3, $4, $5, id, $7 FROM actors
         WHERE id = $6 AND household_id = $2 AND role = 'guest'",
    )
    .bind(id)
    .bind(guardian.household_id)
    .bind(selection.moment_id)
    .bind(selection.revision)
    .bind(selection.include_text)
    .bind(selection.guest_id)
    .bind(guardian.id)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(Failure::Missing);
    }
    let originals = sqlx::query(
        "INSERT INTO relative_share_originals (share_id, household_id, asset_id, link_generation, position)
         SELECT $1, link.household_id, a.id, link.link_generation, link.position
         FROM moment_assets link JOIN family_assets a ON a.id = link.asset_id
              AND a.household_id = link.household_id
         WHERE link.moment_id = $2 AND link.withdrawn_at IS NULL
           AND a.id = ANY($3) AND a.source_url IS NULL",
    )
    .bind(id)
    .bind(selection.moment_id)
    .bind(&selection.original_asset_ids)
    .execute(&mut *tx)
    .await?;
    if originals.rows_affected() as usize != selection.original_asset_ids.len() {
        return Err(Failure::Missing);
    }
    tx.commit().await?;
    Ok(invitation(
        &auth,
        guardian.household_id,
        id,
        StatusCode::CREATED,
    ))
}

fn invitation(auth: &Auth, household: Uuid, id: Uuid, status: StatusCode) -> Response {
    (
        status,
        [(header::LOCATION, format!("/api/v1/shares/{id}"))],
        Json(Invitation {
            id,
            token: invitation_token(auth, household, id),
        }),
    )
        .into_response()
}

fn invitation_token(auth: &Auth, household: Uuid, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(auth.mac(b"relative-share-invitation", &format!("{household}/{id}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptInvitation {
    token: String,
}

async fn accept(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AcceptInvitation>,
) -> Result<StatusCode, Failure> {
    let guest = guest_actor(&auth, &headers, true).await?;
    let id = share_id(&id)?;
    let expected = invitation_token(&auth, guest.household_id, id);
    if !bool::from(expected.as_bytes().ct_eq(input.token.as_bytes())) {
        return Err(Failure::Missing);
    }
    let accepted = sqlx::query(
        "UPDATE relative_shares SET accepted_at = clock_timestamp(),
                access_expires_at = clock_timestamp() + interval '30 days'
         WHERE id = $1 AND guest_id = $2 AND household_id = $3
           AND accepted_at IS NULL AND revoked_at IS NULL AND invitation_expires_at > clock_timestamp()",
    )
    .bind(id)
    .bind(guest.id)
    .bind(guest.household_id)
    .execute(&auth.pool)
    .await?;
    if accepted.rows_affected() != 1 {
        return Err(Failure::Missing);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Failure> {
    let guardian = guardian_actor(&auth, &headers).await?;
    let revoked = sqlx::query(
        "UPDATE relative_shares SET revoked_at = coalesce(revoked_at, clock_timestamp())
         WHERE id = $1 AND household_id = $2",
    )
    .bind(share_id(&id)?)
    .bind(guardian.household_id)
    .execute(&auth.pool)
    .await?;
    if revoked.rows_affected() != 1 {
        return Err(Failure::Missing);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Serialize)]
struct SharedOriginal {
    id: Uuid,
    representation: String,
    label: String,
    uploader_name: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct SharedMoment {
    id: Uuid,
    moment_id: Uuid,
    revision: i32,
    voice: String,
    note: Option<String>,
    activity_date: Option<String>,
    learner_name: String,
    recorder_name: String,
    originals: DbJson<Vec<SharedOriginal>>,
}

async fn read(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<SharedMoment>, Failure> {
    let guest = guest_actor(&auth, &headers, false).await?;
    let shared = sqlx::query_as(
        r#"SELECT s.id, m.id AS moment_id, r.revision, m.voice,
                  CASE WHEN s.include_text THEN r.note END AS note,
                  to_char(r.activity_date, 'YYYY-MM-DD') AS activity_date,
                  learner.display_name AS learner_name, recorder.display_name AS recorder_name,
                  coalesce((
                    SELECT jsonb_agg(jsonb_build_object('id', a.id, 'representation', 'original',
                                      'label', a.label, 'uploader_name', uploader.display_name)
                                     ORDER BY a.position)
                    FROM active_relative_originals a
                    JOIN actors uploader ON uploader.id = a.uploader_id
                    WHERE a.share_id = s.id AND a.household_id = s.household_id
                  ), '[]'::jsonb) AS originals
           FROM active_relative_shares s
           JOIN family_moments m ON m.id = s.moment_id
           JOIN moment_revisions r ON r.moment_id = m.id AND r.revision = s.revision
           JOIN actors learner ON learner.id = m.learner_id
           JOIN actors recorder ON recorder.id = m.recorder_id
           WHERE s.id = $1 AND s.guest_id = $2 AND s.household_id = $3"#,
    )
    .bind(share_id(&id)?)
    .bind(guest.id)
    .bind(guest.household_id)
    .fetch_optional(&auth.pool)
    .await?
    .ok_or(Failure::Missing)?;
    Ok(Json(shared))
}

pub(crate) async fn original(
    auth: &Auth,
    guest: &Actor,
    share: Uuid,
    asset: Uuid,
) -> Result<(String, String), Failure> {
    sqlx::query_as(
        "SELECT a.storage_key, a.label FROM active_relative_shares s
         JOIN active_relative_originals a ON a.share_id = s.id AND a.household_id = s.household_id
         WHERE s.id = $1 AND s.guest_id = $2 AND s.household_id = $3 AND a.id = $4",
    )
    .bind(share)
    .bind(guest.id)
    .bind(guest.household_id)
    .bind(asset)
    .fetch_optional(&auth.pool)
    .await?
    .ok_or(Failure::Missing)
}

pub(crate) async fn guest_actor(
    auth: &Auth,
    headers: &HeaderMap,
    write: bool,
) -> Result<Actor, Failure> {
    let (_, _, session) =
        active_session(auth, headers, write || headers.contains_key(CSRF)).await?;
    if session.actor.role != "guest" || session.diary_learner_id.is_some() {
        return Err(Failure::Missing);
    }
    Ok(session.actor)
}

async fn guardian_actor(auth: &Auth, headers: &HeaderMap) -> Result<Actor, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    if actor.role != "guardian" {
        return Err(Failure::Missing);
    }
    Ok(actor)
}

fn share_id(id: &str) -> Result<Uuid, Failure> {
    id.parse::<Uuid>()
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or(Failure::Missing)
}

#[derive(Serialize, sqlx::FromRow)]
struct ShareSummary {
    id: Uuid,
    activity_date: Option<String>,
}

async fn guest_list(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Vec<ShareSummary>>, Failure> {
    let guest = guest_actor(&auth, &headers, false).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT s.id, to_char(r.activity_date, 'YYYY-MM-DD') AS activity_date
         FROM active_relative_shares s JOIN moment_revisions r
           ON r.moment_id = s.moment_id AND r.revision = s.revision
         WHERE s.household_id = $1 AND s.guest_id = $2
         ORDER BY s.accepted_at DESC, s.id",
        )
        .bind(guest.household_id)
        .bind(guest.id)
        .fetch_all(&auth.pool)
        .await?,
    ))
}

#[derive(Serialize, sqlx::FromRow)]
struct GuestChoice {
    id: Uuid,
    display_name: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct FamilyShare {
    id: Uuid,
    guest_name: String,
    state: String,
    revision: i32,
}

#[derive(Serialize)]
struct Review {
    guests: Vec<GuestChoice>,
    shares: Vec<FamilyShare>,
}

async fn family_review(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Review>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    let id = share_id(&id)?;
    let accessible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM family_moments WHERE id = $1 AND household_id = $2)",
    )
    .bind(id)
    .bind(actor.household_id)
    .fetch_one(&auth.pool)
    .await?;
    if !accessible {
        return Err(Failure::Missing);
    }
    let guests = sqlx::query_as(
        "SELECT id, display_name FROM actors WHERE household_id = $1 AND role = 'guest'
         ORDER BY display_name, id",
    )
    .bind(actor.household_id)
    .fetch_all(&auth.pool)
    .await?;
    let shares = sqlx::query_as(
        "SELECT s.id, a.display_name AS guest_name, s.revision,
                CASE WHEN s.revoked_at IS NOT NULL THEN 'Withdrawn'
                     WHEN coalesce(s.access_expires_at, s.invitation_expires_at) <= statement_timestamp() THEN 'Expired'
                     WHEN s.accepted_at IS NULL THEN 'Invited'
                     ELSE 'Active' END AS state
         FROM relative_shares s JOIN actors a ON a.id = s.guest_id
         WHERE s.moment_id = $1 AND s.household_id = $2 ORDER BY s.invitation_expires_at, s.id",
    ).bind(id).bind(actor.household_id).fetch_all(&auth.pool).await?;
    Ok(Json(Review { guests, shares }))
}

async fn thread_actor(auth: &Auth, headers: &HeaderMap, write: bool) -> Result<Actor, Failure> {
    let (_, _, session) =
        active_session(auth, headers, write || headers.contains_key(CSRF)).await?;
    if session.diary_learner_id.is_some() {
        return Err(Failure::Missing);
    }
    Ok(session.actor)
}

async fn thread(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
) -> Result<Uuid, Failure> {
    sqlx::query_scalar(
        "SELECT moment_id FROM relative_shares
         WHERE id = $1 AND household_id = $2 AND (NOT $3 OR guest_id = $4)
           AND accepted_at IS NOT NULL AND revoked_at IS NULL AND access_expires_at > clock_timestamp()
         FOR UPDATE",
    )
    .bind(id).bind(actor.household_id).bind(actor.role == "guest").bind(actor.id)
    .fetch_optional(&mut **tx).await?.ok_or(Failure::Missing)
}

async fn list_comments(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<Comment>>, Failure> {
    let actor = thread_actor(&auth, &headers, false).await?;
    let id = share_id(&id)?;
    let mut tx = auth.pool.begin().await?;
    // Hold the grant lock so the conversation read and withdrawal have a defined order.
    let moment = thread(&mut tx, &actor, id).await?;
    let comments = scoped_comments(&mut *tx, &actor, moment, id).await?;
    tx.commit().await?;
    Ok(Json(comments))
}

async fn add_comment(
    State(auth): State<Auth>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AddComment>,
) -> Result<Response, Failure> {
    let actor = thread_actor(&auth, &headers, true).await?;
    let id = share_id(&id)?;
    let mut tx = auth.pool.begin().await?;
    let moment = thread(&mut tx, &actor, id).await?;
    let saved = add_scoped_comment(&mut tx, &actor, moment, Some(id), &input).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(saved)).into_response())
}

async fn withdraw_comment(
    State(auth): State<Auth>,
    Path((id, comment)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, Failure> {
    change_comment(&auth, &headers, share_id(&id)?, share_id(&comment)?, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_comment(
    State(auth): State<Auth>,
    Path((id, comment)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Comment>, Failure> {
    Ok(Json(
        change_comment(&auth, &headers, share_id(&id)?, share_id(&comment)?, false).await?,
    ))
}

async fn change_comment(
    auth: &Auth,
    headers: &HeaderMap,
    id: Uuid,
    comment: Uuid,
    withdraw: bool,
) -> Result<Comment, Failure> {
    let actor = thread_actor(auth, headers, true).await?;
    let mut tx = auth.pool.begin().await?;
    let moment = thread(&mut tx, &actor, id).await?;
    let updated =
        set_scoped_withdrawal(&mut tx, &actor, moment, Some(id), comment, withdraw).await?;
    tx.commit().await?;
    Ok(updated)
}

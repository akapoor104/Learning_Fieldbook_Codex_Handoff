use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::{routes::family_actor, Auth, Failure};

use super::{moment_id, store::archive};

pub(super) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/assets/links", post(create_link))
        .route(
            "/api/v1/moments/{moment}/attachments/{asset}",
            put(attach_link).delete(detach_link),
        )
        .route(
            "/api/v1/moments/{moment}/attachments/{asset}/copy",
            put(keep_copy).delete(withdraw_copy),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewLink {
    submission_key: Uuid,
    label: String,
    source_url: String,
}

#[derive(Serialize)]
struct LinkId {
    id: Uuid,
}

async fn create_link(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<NewLink>,
) -> Result<(StatusCode, Json<LinkId>), Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = archive::create_link(
        &auth.pool,
        &actor,
        input.submission_key,
        &input.label,
        &input.source_url,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(LinkId { id })))
}

async fn attach_link(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(ids): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    set_link(&auth, &headers, ids, true).await
}

async fn detach_link(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(ids): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    set_link(&auth, &headers, ids, false).await
}

async fn set_link(
    auth: &Auth,
    headers: &HeaderMap,
    (moment, asset): (String, String),
    attached: bool,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    archive::set_link(
        &auth.pool,
        &actor,
        moment_id(&moment)?,
        moment_id(&asset)?,
        attached,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyInput {
    original_id: Uuid,
}

async fn keep_copy(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(ids): Path<(String, String)>,
    Json(input): Json<CopyInput>,
) -> Result<StatusCode, Failure> {
    set_copy(&auth, &headers, ids, input, true).await
}

async fn withdraw_copy(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(ids): Path<(String, String)>,
    Json(input): Json<CopyInput>,
) -> Result<StatusCode, Failure> {
    set_copy(&auth, &headers, ids, input, false).await
}

async fn set_copy(
    auth: &Auth,
    headers: &HeaderMap,
    (moment, asset): (String, String),
    input: CopyInput,
    kept: bool,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(auth, headers, true).await?;
    archive::set_copy(
        &auth.pool,
        &actor,
        moment_id(&moment)?,
        moment_id(&asset)?,
        input.original_id,
        kept,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

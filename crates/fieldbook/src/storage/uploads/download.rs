use std::fmt::Write;

use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    Extension,
};
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::identity::{routes::family_actor, Auth, Failure};

use super::{files::original_key, upload_id, PrivateMedia};

pub(super) async fn original(
    State(auth): State<Auth>,
    Extension(media): Extension<Option<PrivateMedia>>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, request.headers(), false).await?;
    let id = upload_id(&id)?;
    let (original, key, label): (Uuid, String, String) = sqlx::query_as(
        "SELECT original.id, original.storage_key, original.label
         FROM assets a JOIN assets original ON original.id = coalesce(a.copy_asset_id, a.id)
             AND original.household_id = a.household_id AND original.source_url IS NULL
         WHERE a.id = $1 AND a.household_id = $2 AND (
             EXISTS (
                 SELECT 1 FROM moment_assets ma JOIN moments m ON m.id = ma.moment_id
                     AND m.household_id = ma.household_id
                 WHERE ma.asset_id = a.id AND ma.household_id = a.household_id
                     AND m.visibility = 'family' AND ma.withdrawn_at IS NULL
             ) OR EXISTS (
                 SELECT 1 FROM family_photo_inbox fi
                 WHERE fi.asset_id = a.id AND fi.household_id = a.household_id
                     AND ($3 OR fi.learner_id = $4)
                     AND NOT EXISTS (SELECT 1 FROM moment_assets ma WHERE ma.asset_id = fi.asset_id)
             )
         )",
    )
    .bind(id)
    .bind(actor.household_id)
    .bind(actor.role == "guardian")
    .bind(actor.id)
    .fetch_optional(&auth.pool)
    .await?
    .ok_or(Failure::Missing)?;
    if key != original_key(original) {
        return Err(Failure::Missing);
    }

    let path = media.ok_or(Failure::Unavailable)?.original(original);
    let mut response = ServeFile::new(path)
        .try_call(request)
        .await
        .map_err(|_| Failure::Unavailable)?
        .into_response();
    if response.status().is_success() {
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        headers.insert(header::CONTENT_DISPOSITION, attachment_header(&label)?);
    }
    Ok(response)
}

fn attachment_header(label: &str) -> Result<HeaderValue, Failure> {
    let mut value = String::from("attachment; filename*=UTF-8''");
    for byte in label.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            value.push(char::from(byte));
        } else {
            write!(value, "%{byte:02X}").map_err(|_| Failure::Unavailable)?;
        }
    }
    HeaderValue::from_str(&value).map_err(|_| Failure::Unavailable)
}

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, FromRequest, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Extension, Json, Router,
};
use uuid::Uuid;

use crate::identity::{routes::family_actor, Auth, Failure};

mod files;
mod store;
#[cfg(test)]
mod tests;

pub use files::PrivateMedia;
pub use store::sweep;
use store::{Manifest, Upload, PART_BYTES};

pub(crate) fn router(media: Option<PrivateMedia>) -> Router<Auth> {
    let router = Router::new()
        .route("/api/v1/uploads", post(begin))
        .route("/api/v1/uploads/{id}", get(status))
        .route(
            "/api/v1/uploads/{id}/parts/{offset}",
            put(append).layer(DefaultBodyLimit::max(PART_BYTES)),
        )
        .route("/api/v1/uploads/{id}/finalize", post(finish));
    router.layer(Extension(media))
}

async fn begin(
    State(auth): State<Auth>,
    Extension(media): Extension<Option<PrivateMedia>>,
    headers: HeaderMap,
    Json(manifest): Json<Manifest>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    media.ok_or(Failure::Unavailable)?;
    let upload = store::begin(&auth.pool, &actor, manifest).await?;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/api/v1/uploads/{}", upload.id))],
        Json(upload),
    )
        .into_response())
}

async fn status(
    State(auth): State<Auth>,
    Extension(media): Extension<Option<PrivateMedia>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Upload>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    media.ok_or(Failure::Unavailable)?;
    Ok(Json(
        store::status(&auth.pool, &actor, upload_id(&id)?).await?,
    ))
}

async fn append(
    State(auth): State<Auth>,
    Extension(media): Extension<Option<PrivateMedia>>,
    Path((id, offset)): Path<(String, i64)>,
    request: Request,
) -> Result<Response, Failure> {
    let headers = request.headers().clone();
    family_actor(&auth, &headers, true).await?;
    let id = upload_id(&id)?;
    let media = media.ok_or(Failure::Unavailable)?;
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some("application/octet-stream")
    {
        return Ok(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response());
    }
    let bytes = match Bytes::from_request(request, &()).await {
        Ok(bytes) => bytes,
        Err(error) => return Ok(error.into_response()),
    };
    let actor = family_actor(&auth, &headers, true).await?;
    // Hold the row lock through disk I/O even when the HTTP caller disconnects.
    let upload =
        tokio::spawn(
            async move { store::append(&auth.pool, &actor, media, id, offset, bytes).await },
        )
        .await
        .map_err(|_| Failure::Unavailable)??;
    Ok(Json(upload).into_response())
}

async fn finish(
    State(auth): State<Auth>,
    Extension(media): Extension<Option<PrivateMedia>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Upload>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = upload_id(&id)?;
    let media = media.ok_or(Failure::Unavailable)?;
    let upload = tokio::spawn(async move { store::finish(&auth.pool, &actor, media, id).await })
        .await
        .map_err(|_| Failure::Unavailable)??;
    Ok(Json(upload))
}

fn upload_id(id: &str) -> Result<Uuid, Failure> {
    id.parse().map_err(|_| Failure::Missing)
}

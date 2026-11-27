use axum::{
    extract::{Form, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use time::{macros::format_description, Date};
use uuid::Uuid;

use crate::identity::{diary::read_scope, routes::family_actor, Actor, Auth, Failure, CSRF};

mod store;
#[cfg(test)]
mod tests;

pub(crate) fn router() -> Router<Auth> {
    Router::new()
        .route("/api/v1/moments", get(list).post(create))
        .route("/api/v1/diary", get(diary))
        .route("/api/v1/diary/{id}", get(diary_entry))
        .route("/moments/new/", post(create_form))
        .route("/api/v1/moments/{id}", get(read).patch(edit))
        .route("/api/v1/moments/{id}/revisions", get(revisions))
        .route("/api/v1/moments/{id}/attachments", get(moment_attachments))
        .merge(archive::router())
        .route("/api/v1/family/photo-inbox/{learner}", get(photo_inbox))
        .route(
            "/api/v1/family/photo-inbox/{learner}/{asset}",
            axum::routing::put(add_inbox_asset),
        )
        .route(
            "/api/v1/family/photo-inbox/{asset}/match/{moment}",
            post(match_asset).delete(unmatch_asset),
        )
        .route("/api/v1/journal/learners", get(learners))
        .route(
            "/api/v1/moments/{id}/confirmation",
            post(confirm).delete(withdraw_confirmation),
        )
        .route(
            "/api/v1/moments/{id}/comments",
            get(comments).post(add_comment),
        )
        .route(
            "/api/v1/moments/{id}/comments/{comment}",
            get(comment).delete(withdraw_comment),
        )
        .route(
            "/api/v1/moments/{id}/comments/{comment}/restore",
            post(restore_comment),
        )
}

mod archive;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateMoment {
    submission_key: Uuid,
    learner_id: Option<Uuid>,
    #[serde(default)]
    scribed: bool,
    note: Option<String>,
    activity_date: Option<String>,
    #[serde(default)]
    asset_ids: Vec<Uuid>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EditMoment {
    expected_revision: i32,
    note: Option<String>,
    activity_date: Option<String>,
}

fn activity_date(date: Option<&str>) -> Result<Option<Date>, Failure> {
    date.map(|value| {
        if value.len() != 10 {
            return Err(Failure::Validation("invalid_activity_date"));
        }
        Date::parse(value, format_description!("[year]-[month]-[day]"))
            .ok()
            .filter(|date| date.year() > 0)
            .ok_or(Failure::Validation("invalid_activity_date"))
    })
    .transpose()
}

fn normalized_note(note: Option<&str>) -> Result<Option<&str>, Failure> {
    if note.is_some_and(|text| text.len() > 16_000 || text.contains('\0')) {
        return Err(Failure::Validation("invalid_note"));
    }
    Ok(note.filter(|text| !text.trim().is_empty()))
}

fn moment_id(id: &str) -> Result<Uuid, Failure> {
    id.parse().map_err(|_| Failure::Missing)
}

async fn create(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<CreateMoment>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let moment = store::create(&auth.pool, &actor, &input).await?;
    let location = format!("/api/v1/moments/{}", moment.id);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(moment),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MomentForm {
    csrf_token: String,
    submission_key: Uuid,
    learner_id: Option<Uuid>,
    note: Option<String>,
    activity_date: Option<String>,
    asset_ids: Option<String>,
    #[serde(default)]
    scribed: bool,
}

async fn create_form(
    State(auth): State<Auth>,
    mut headers: HeaderMap,
    Form(form): Form<MomentForm>,
) -> Result<Response, Failure> {
    if headers.contains_key(CSRF) {
        return Err(Failure::Forbidden);
    }
    headers.insert(
        CSRF,
        HeaderValue::from_str(&form.csrf_token).map_err(|_| Failure::Forbidden)?,
    );
    let actor = family_actor(&auth, &headers, true).await?;
    let asset_ids = form
        .asset_ids
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| value.split(',').map(moment_id).collect::<Result<_, _>>())
        .transpose()?
        .unwrap_or_default();
    let input = CreateMoment {
        submission_key: form.submission_key,
        learner_id: form.learner_id,
        note: form.note,
        activity_date: form.activity_date.filter(|value| !value.is_empty()),
        scribed: form.scribed,
        asset_ids,
    };
    match store::create(&auth.pool, &actor, &input).await {
        Ok(moment) => Ok(Redirect::to(&format!("/?moment={}", moment.id)).into_response()),
        Err(Failure::Validation(error)) => Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": error,
                "note": input.note,
                "activity_date": input.activity_date,
            })),
        )
            .into_response()),
        Err(failure) => Err(failure),
    }
}

async fn list(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Vec<store::Moment>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(store::list(&auth.pool, &actor).await?))
}

async fn diary(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Vec<store::Moment>>, Failure> {
    let scope = read_scope(&auth, &headers).await?;
    Ok(Json(store::diary(&auth.pool, &scope).await?))
}

async fn diary_entry(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<store::Moment>, Failure> {
    let scope = read_scope(&auth, &headers).await?;
    Ok(Json(
        store::diary_entry(&auth.pool, &scope, moment_id(&id)?).await?,
    ))
}

async fn read(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<store::Moment>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        store::read(&auth.pool, &actor, moment_id(&id)?).await?,
    ))
}

async fn edit(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<EditMoment>,
) -> Result<Json<store::Moment>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    Ok(Json(
        store::edit(&auth.pool, &actor, moment_id(&id)?, &input).await?,
    ))
}

async fn revisions(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<store::Moment>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        store::history(&auth.pool, &actor, moment_id(&id)?).await?,
    ))
}

async fn learners(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Vec<store::Learner>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(store::learners(&auth.pool, &actor).await?))
}

use store::photos::{self, Attachment};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddInboxAsset {
    activity_date: Option<String>,
}

async fn photo_inbox(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(learner): Path<String>,
) -> Result<Json<Vec<Attachment>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        photos::list(&auth.pool, &actor, moment_id(&learner)?).await?,
    ))
}

async fn add_inbox_asset(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((learner, asset)): Path<(String, String)>,
    Json(input): Json<AddInboxAsset>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    photos::add(
        &auth.pool,
        &actor,
        moment_id(&learner)?,
        moment_id(&asset)?,
        activity_date(input.activity_date.as_deref())?,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn moment_attachments(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(moment): Path<String>,
) -> Result<Json<Vec<Attachment>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        photos::linked(&auth.pool, &actor, moment_id(&moment)?).await?,
    ))
}

async fn match_asset(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((asset, moment)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    photos::set_match(
        &auth.pool,
        &actor,
        moment_id(&moment)?,
        moment_id(&asset)?,
        true,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unmatch_asset(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((asset, moment)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    photos::set_match(
        &auth.pool,
        &actor,
        moment_id(&moment)?,
        moment_id(&asset)?,
        false,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmMoment {
    expected_revision: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddComment {
    submission_key: Uuid,
    kind: String,
    body: String,
}

use store::contributions::{self, Comment};

async fn confirm(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ConfirmMoment>,
) -> Result<Json<store::Moment>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    Ok(Json(
        contributions::set_confirmation(
            &auth.pool,
            &actor,
            moment_id(&id)?,
            input.expected_revision,
            true,
        )
        .await?,
    ))
}

async fn withdraw_confirmation(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ConfirmMoment>,
) -> Result<Json<store::Moment>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    Ok(Json(
        contributions::set_confirmation(
            &auth.pool,
            &actor,
            moment_id(&id)?,
            input.expected_revision,
            false,
        )
        .await?,
    ))
}

async fn comments(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<Comment>>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        contributions::list_comments(&auth.pool, &actor, moment_id(&id)?).await?,
    ))
}

async fn comment(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, comment)): Path<(String, String)>,
) -> Result<Json<Comment>, Failure> {
    let actor = family_actor(&auth, &headers, false).await?;
    Ok(Json(
        contributions::comment(&auth.pool, &actor, moment_id(&id)?, moment_id(&comment)?).await?,
    ))
}

async fn add_comment(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AddComment>,
) -> Result<Response, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let id = moment_id(&id)?;
    let comment = contributions::add_comment(&auth.pool, &actor, id, &input).await?;
    let location = format!("/api/v1/moments/{id}/comments/{}", comment.id);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(comment),
    )
        .into_response())
}

async fn withdraw_comment(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, comment)): Path<(String, String)>,
) -> Result<StatusCode, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    contributions::set_withdrawal(
        &auth.pool,
        &actor,
        moment_id(&id)?,
        moment_id(&comment)?,
        true,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_comment(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path((id, comment)): Path<(String, String)>,
) -> Result<Json<Comment>, Failure> {
    let actor = family_actor(&auth, &headers, true).await?;
    let (id, comment) = (moment_id(&id)?, moment_id(&comment)?);
    contributions::set_withdrawal(&auth.pool, &actor, id, comment, false).await?;
    Ok(Json(
        contributions::comment(&auth.pool, &actor, id, comment).await?,
    ))
}

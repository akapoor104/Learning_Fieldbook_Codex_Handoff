use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use webauthn_rs::prelude::{Passkey, PublicKeyCredential, RegisterPublicKeyCredential};

use super::{
    cookie, hash_token, one_header, random_token, set_cookie,
    store::{self, Purpose, Session},
    token_format, Actor, Auth, Failure, CSRF,
};

#[derive(Deserialize)]
pub(super) struct Enrollment {
    token: String,
}

#[derive(Deserialize)]
pub(super) struct SignIn {
    username: String,
}

#[derive(Deserialize)]
pub(super) struct Finish<T> {
    ceremony: String,
    credential: T,
}

async fn require_signed_out(auth: &Auth, headers: &HeaderMap) -> Result<(), Failure> {
    if let Some(raw) = cookie(headers) {
        if store::session(&auth.pool, &auth.session_hash(raw))
            .await?
            .is_some()
        {
            return Err(Failure::Conflict);
        }
    }
    Ok(())
}

async fn active_session<'a>(
    auth: &Auth,
    headers: &'a HeaderMap,
    require_csrf: bool,
) -> Result<(&'a str, Vec<u8>, Session), Failure> {
    let raw = cookie(headers).ok_or(Failure::Unauthorized)?;
    if require_csrf
        && !one_header(headers, CSRF)
            .is_some_and(|value| bool::from(value.as_bytes().ct_eq(auth.csrf(raw).as_bytes())))
    {
        return Err(Failure::Forbidden);
    }
    let hash = auth.session_hash(raw);
    let session = store::session(&auth.pool, &hash)
        .await?
        .ok_or(Failure::Unauthorized)?;
    Ok((raw, hash, session))
}

pub(super) async fn enroll_start(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<Enrollment>,
) -> Result<Json<Value>, Failure> {
    require_signed_out(&auth, &headers).await?;
    if !token_format(&input.token) {
        return Err(Failure::Unauthorized);
    }
    let actor = store::enrollment_actor(&auth.pool, &hash_token(&input.token)).await?;
    let (options, state) = auth
        .webauthn
        .start_passkey_registration(actor.id, &actor.username, &actor.display_name, None)
        .map_err(|_| Failure::Unavailable)?;
    let ceremony = store::start_ceremony(
        &auth.pool,
        &actor,
        Purpose::Enroll,
        None,
        serde_json::to_value(state).map_err(|_| Failure::Unavailable)?,
    )
    .await?;
    Ok(Json(json!({ "ceremony": ceremony, "options": options })))
}

pub(super) async fn enroll_finish(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<StatusCode, Failure> {
    require_signed_out(&auth, &headers).await?;
    let ceremony = take_ceremony(&auth, &input.ceremony, Purpose::Enroll, None).await?;
    let state = serde_json::from_value(ceremony.challenge_state.clone())
        .map_err(|_| Failure::Unavailable)?;
    let passkey = auth
        .webauthn
        .finish_passkey_registration(&input.credential, &state)
        .map_err(|_| Failure::Unauthorized)?;
    store::save_enrollment(&auth.pool, &ceremony, &passkey).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn sign_in_start(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<SignIn>,
) -> Result<Json<Value>, Failure> {
    require_signed_out(&auth, &headers).await?;
    let (actor, passkey) = store::login_actor(&auth.pool, &input.username).await?;
    start_assertion(&auth, &actor, passkey, Purpose::SignIn, None).await
}

pub(super) async fn sign_in_finish(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<Finish<PublicKeyCredential>>,
) -> Result<Response, Failure> {
    require_signed_out(&auth, &headers).await?;
    finish_assertion(&auth, input, Purpose::SignIn, None).await
}

pub(super) async fn step_up_start(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<Value>, Failure> {
    let (_, hash, session) = active_session(&auth, &headers, true).await?;
    if session.actor.role != "guardian" {
        return Err(Failure::Forbidden);
    }
    let passkey = store::passkey(&auth.pool, session.actor.id).await?;
    start_assertion(&auth, &session.actor, passkey, Purpose::StepUp, Some(&hash)).await
}

pub(super) async fn step_up_finish(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Json(input): Json<Finish<PublicKeyCredential>>,
) -> Result<Response, Failure> {
    let (_, hash, session) = active_session(&auth, &headers, true).await?;
    if session.actor.role != "guardian" {
        return Err(Failure::Forbidden);
    }
    finish_assertion(&auth, input, Purpose::StepUp, Some(&hash)).await
}

async fn start_assertion(
    auth: &Auth,
    actor: &Actor,
    passkey: Passkey,
    purpose: Purpose,
    session_hash: Option<&[u8]>,
) -> Result<Json<Value>, Failure> {
    let (options, state) = auth
        .webauthn
        .start_passkey_authentication(&[passkey])
        .map_err(|_| Failure::Unavailable)?;
    let ceremony = store::start_ceremony(
        &auth.pool,
        actor,
        purpose,
        session_hash,
        serde_json::to_value(state).map_err(|_| Failure::Unavailable)?,
    )
    .await?;
    Ok(Json(json!({ "ceremony": ceremony, "options": options })))
}

async fn take_ceremony(
    auth: &Auth,
    token: &str,
    purpose: Purpose,
    session_hash: Option<&[u8]>,
) -> Result<store::Ceremony, Failure> {
    if !token_format(token) {
        return Err(Failure::Invalid);
    }
    store::take_ceremony(&auth.pool, token, purpose, session_hash).await
}

async fn finish_assertion(
    auth: &Auth,
    input: Finish<PublicKeyCredential>,
    purpose: Purpose,
    previous: Option<&[u8]>,
) -> Result<Response, Failure> {
    let ceremony = take_ceremony(auth, &input.ceremony, purpose, previous).await?;
    let state = serde_json::from_value(ceremony.challenge_state.clone())
        .map_err(|_| Failure::Unavailable)?;
    let result = auth
        .webauthn
        .finish_passkey_authentication(&input.credential, &state)
        .map_err(|_| Failure::Unauthorized)?;
    let mut tx = auth.pool.begin().await?;
    let actor = store::current_actor(&mut tx, &ceremony).await?;
    let mut passkey = store::passkey_in_transaction(&mut tx, actor.id).await?;
    passkey
        .update_credential(&result)
        .ok_or(Failure::Unauthorized)?;
    store::save_assertion(&mut tx, actor.id, result.counter(), &passkey).await?;
    let raw = random_token();
    let session = store::new_session(&mut tx, &actor, &auth.session_hash(&raw), previous).await?;
    tx.commit().await?;
    let mut response = session_response(auth, &raw, session).into_response();
    set_cookie(&mut response, &raw, 8 * 60 * 60);
    Ok(response)
}

#[derive(Serialize)]
pub(super) struct SessionResponse {
    #[serde(flatten)]
    session: Session,
    csrf_token: String,
}

fn session_response(auth: &Auth, raw: &str, session: Session) -> Json<SessionResponse> {
    Json(SessionResponse {
        session,
        csrf_token: auth.csrf(raw),
    })
}

pub(super) async fn session(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, Failure> {
    let (raw, _, session) = active_session(&auth, &headers, false).await?;
    Ok(session_response(&auth, raw, session))
}

pub(super) async fn keep_alive(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, Failure> {
    let (raw, hash, _) = active_session(&auth, &headers, true).await?;
    store::refresh(&auth.pool, &hash).await?;
    let session = store::session(&auth.pool, &hash)
        .await?
        .ok_or(Failure::Unauthorized)?;
    Ok(session_response(&auth, raw, session))
}

pub(super) async fn sign_out(
    State(auth): State<Auth>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Some(raw) = cookie(&headers) {
        let hash = auth.session_hash(raw);
        if store::session(&auth.pool, &hash).await?.is_some() {
            active_session(&auth, &headers, true).await?;
            store::delete_session(&auth.pool, &hash).await?;
        }
    }
    set_cookie(&mut response, "", 0);
    Ok(response)
}

pub(super) async fn household(
    State(auth): State<Auth>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Failure> {
    let (_, _, session) = active_session(&auth, &headers, false).await?;
    let id: Uuid = id.parse().map_err(|_| Failure::Missing)?;
    if session.actor.household_id != id || session.actor.role == "guest" {
        return Err(Failure::Missing);
    }
    let label: String = sqlx::query_scalar("SELECT label FROM households WHERE id = $1")
        .bind(id)
        .fetch_optional(&auth.pool)
        .await?
        .ok_or(Failure::Missing)?;
    Ok(Json(json!({ "id": id, "label": label })))
}

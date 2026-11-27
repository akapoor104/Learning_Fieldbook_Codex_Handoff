use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use serde_json::json;
use sha2::Sha256;
use sqlx::PgPool;
use std::{io, sync::Arc};
use url::Url;
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

pub(crate) mod routes;
pub(crate) mod store;

pub use store::{issue_enrollment, provision_actor, revoke_credentials};

const COOKIE: &str = "__Host-fieldbook";
pub(crate) const CSRF: &str = "x-csrf-token";
const IDLE_SECONDS: i32 = 15 * 60;

#[derive(Clone)]
pub(crate) struct Auth {
    pub(crate) pool: PgPool,
    webauthn: Arc<Webauthn>,
    origin: String,
    authority: String,
    secret: Arc<[u8]>,
}

pub fn router(
    pool: PgPool,
    origin: &str,
    rp: &str,
    secret: &str,
    web_root: std::path::PathBuf,
) -> io::Result<Router> {
    router_with_media(pool, origin, rp, secret, web_root, None)
}

pub fn router_with_media(
    pool: PgPool,
    origin: &str,
    rp: &str,
    secret: &str,
    web_root: std::path::PathBuf,
    media: Option<crate::storage::PrivateMedia>,
) -> io::Result<Router> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidInput, message);
    let url = Url::parse(origin).map_err(|_| invalid("invalid FIELD_PUBLIC_ORIGIN"))?;
    if url.origin().ascii_serialization() != origin
        || !(url.scheme() == "https"
            || (url.scheme() == "http" && url.host_str() == Some("localhost")))
        || url.host_str() != Some(rp)
    {
        return Err(invalid("set a canonical origin and matching FIELD_RP_ID"));
    }
    if secret.trim().len() < 32 {
        return Err(invalid(
            "set FIELD_SESSION_SECRET to at least 32 characters",
        ));
    }
    let webauthn = WebauthnBuilder::new(rp, &url)
        .and_then(|builder| builder.rp_name("Learning Fieldbook").build())
        .map_err(|_| invalid("invalid WebAuthn origin or relying party ID"))?;
    let state = Auth {
        pool,
        webauthn: Arc::new(webauthn),
        origin: origin.to_owned(),
        authority: origin[url.scheme().len() + 3..].to_owned(),
        secret: Arc::from(secret.as_bytes()),
    };
    Ok(Router::new()
        .route("/api/v1/auth/enroll/start", post(routes::enroll_start))
        .route("/api/v1/auth/enroll/finish", post(routes::enroll_finish))
        .route("/api/v1/auth/sign-in/start", post(routes::sign_in_start))
        .route("/api/v1/auth/sign-in/finish", post(routes::sign_in_finish))
        .route("/api/v1/auth/step-up/start", post(routes::step_up_start))
        .route("/api/v1/auth/step-up/finish", post(routes::step_up_finish))
        .route(
            "/api/v1/session",
            get(routes::session).delete(routes::sign_out),
        )
        .route("/api/v1/session/keep-alive", post(routes::keep_alive))
        .route("/api/v1/households/{id}", get(routes::household))
        .merge(crate::journal::router())
        .merge(crate::storage::uploads::router(media))
        .with_state(state.clone())
        .merge(crate::web::router(web_root))
        .layer(middleware::from_fn_with_state(state, boundary)))
}

async fn boundary(
    axum::extract::State(auth): axum::extract::State<Auth>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .uri()
        .authority()
        .map(|value| value.as_str())
        .or_else(|| one_header(request.headers(), header::HOST.as_str()));
    let safe =
        request.method() == axum::http::Method::GET || request.method() == axum::http::Method::HEAD;
    let mut response = if host != Some(&auth.authority)
        || (!safe && one_header(request.headers(), header::ORIGIN.as_str()) != Some(&auth.origin))
    {
        Failure::Forbidden.into_response()
    } else {
        next.run(request).await
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    response
}

fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(first)
}

fn random_token() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn token_format(value: &str) -> bool {
    value.len() == 43
        && URL_SAFE_NO_PAD
            .decode(value)
            .is_ok_and(|bytes| bytes.len() == 32)
}

fn hash_token(value: &str) -> Vec<u8> {
    use sha2::Digest;
    Sha256::digest(value.as_bytes()).to_vec()
}

impl Auth {
    fn session_hash(&self, raw: &str) -> Vec<u8> {
        self.mac(b"session", raw)
    }

    fn csrf(&self, raw: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.mac(b"csrf", raw))
    }

    fn mac(&self, purpose: &[u8], raw: &str) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret).expect("HMAC accepts any key");
        mac.update(purpose);
        mac.update(&[0]);
        mac.update(raw.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }
}

fn cookie(headers: &HeaderMap) -> Option<&str> {
    let mut cookies = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE);
    let raw = cookies.next()?.1;
    (cookies.next().is_none() && token_format(raw)).then_some(raw)
}

fn set_cookie(response: &mut Response, raw: &str, max_age: i32) {
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{COOKIE}={raw}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={max_age}"
        ))
        .expect("cookie values are generated by the server"),
    );
}

#[derive(Debug)]
pub(crate) enum Failure {
    Invalid,
    Unauthorized,
    Forbidden,
    Missing,
    Conflict,
    Unavailable,
    Validation(&'static str),
    Stale {
        saved: serde_json::Value,
        submitted: serde_json::Value,
    },
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "sign_in_required"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::Missing => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict => (StatusCode::CONFLICT, "conflict"),
            Self::Validation(code) => (StatusCode::UNPROCESSABLE_ENTITY, code),
            Self::Stale { saved, submitted } => {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "stale_revision", "saved": saved, "submitted": submitted })),
                ).into_response();
            }
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
        (status, Json(json!({ "error": code }))).into_response()
    }
}

impl From<sqlx::Error> for Failure {
    fn from(error: sqlx::Error) -> Self {
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            Self::Conflict
        } else {
            Self::Unavailable
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Actor {
    pub(crate) id: uuid::Uuid,
    pub(crate) household_id: uuid::Uuid,
    username: String,
    display_name: String,
    pub(crate) role: String,
    #[serde(skip)]
    credential_generation: i64,
}

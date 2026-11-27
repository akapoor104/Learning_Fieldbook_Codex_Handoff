use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;

pub mod storage;

pub fn router(pool: Option<PgPool>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(readiness))
        .with_state(pool)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn readiness(State(pool): State<Option<PgPool>>) -> (StatusCode, Json<Value>) {
    if let Some(pool) = pool {
        if storage::is_ready(&pool).await {
            return (StatusCode::OK, health().await);
        }
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "status": "unavailable" })),
    )
}

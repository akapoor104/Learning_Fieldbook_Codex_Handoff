use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router {
    Router::new().route("/healthz", get(health))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

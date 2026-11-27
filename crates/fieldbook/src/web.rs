use std::path::PathBuf;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use tower_http::services::ServeDir;

pub(crate) fn router(root: PathBuf) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/diary/", get(index))
        .with_state(root.join("index.html"))
        .nest_service(
            "/assets",
            ServeDir::new(root.join("assets")).append_index_html_on_directories(false),
        )
}

async fn index(State(index): State<PathBuf>) -> impl IntoResponse {
    match tokio::fs::read_to_string(index).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

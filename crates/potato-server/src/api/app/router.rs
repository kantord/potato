use axum::{Router, routing::post};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

use super::endpoints;
use super::state::AppState;

pub fn app_router(static_dir: PathBuf, container_id: Option<String>) -> Router {
    let gui_dir = static_dir.join("app/gui");
    let state = AppState {
        container_id,
        static_dir: static_dir.clone(),
        stdin_writers: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/calls", post(endpoints::call::handler))
        .route("/calls/{id}/stdin", post(endpoints::stdin::handler))
        .route("/render", post(endpoints::render::handler))
        .nest_service("/files", ServeDir::new(gui_dir))
        .with_state(state)
}

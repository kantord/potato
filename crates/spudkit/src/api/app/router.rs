use axum::{
    Router,
    routing::{get, post},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::endpoints;
use super::state::AppState;
use crate::container::AppContainer;

pub fn app_router(container_id: String) -> Router {
    let state = AppState {
        container: AppContainer { id: container_id },
        stdin_writers: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/calls", post(endpoints::call::handler))
        .route("/calls/{id}/stdin", post(endpoints::stdin::handler))
        .route("/render/{script}", post(endpoints::render::handler))
        .route("/files/{*path}", get(endpoints::files::handler))
        .with_state(state)
}

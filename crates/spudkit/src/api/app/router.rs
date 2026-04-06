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

pub fn app_router(container: AppContainer) -> Router {
    let state = AppState {
        container,
        stdin_writers: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/_api/calls", post(endpoints::call::handler))
        .route("/_api/calls/ws", get(endpoints::ws_call::handler))
        .route("/_api/calls/{id}/stdin", post(endpoints::stdin::handler))
        .route("/_api/stdin-ws", get(endpoints::ws_stdin::handler))
        .route("/_api/render/{script}", post(endpoints::render::handler))
        .route("/_api/files/{*path}", get(endpoints::files::handler))
        .fallback(get(endpoints::files::fallback))
        .with_state(state)
}

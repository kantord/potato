use axum::extract::Path;
use axum::{Json, extract::State};

use super::super::state::AppState;

#[derive(serde::Deserialize)]
pub(crate) struct StdinRequest {
    data: serde_json::Value,
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    Path(call_id): Path<String>,
    Json(body): Json<StdinRequest>,
) -> Json<serde_json::Value> {
    let line = serde_json::to_string(&body.data).unwrap() + "\n";

    if state.write_stdin(&call_id, line.as_bytes()).await {
        Json(serde_json::json!({"ok": true}))
    } else {
        Json(serde_json::json!({"ok": false, "error": "call not found or not started"}))
    }
}

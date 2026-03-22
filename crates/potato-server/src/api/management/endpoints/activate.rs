use axum::{Json, extract::State};

use crate::app_manager::AppManager;

#[derive(serde::Deserialize)]
pub(crate) struct ActivateRequest {
    image: String,
}

pub(crate) async fn handler(
    State(manager): State<AppManager>,
    Json(body): Json<ActivateRequest>,
) -> Json<serde_json::Value> {
    match manager.activate(&body.image).await {
        Ok(status) => Json(serde_json::json!({"ok": true, "status": status})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

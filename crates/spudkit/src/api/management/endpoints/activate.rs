use axum::{Json, extract::State};
use spudkit_core::Spud;

use crate::app_manager::AppManager;

#[derive(serde::Deserialize)]
pub(crate) struct ActivateRequest {
    name: String,
}

pub(crate) async fn handler(
    State(manager): State<AppManager>,
    Json(body): Json<ActivateRequest>,
) -> Json<serde_json::Value> {
    let spud = match Spud::new(&body.name) {
        Ok(s) => s,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };
    match manager.activate(&spud).await {
        Ok(status) => Json(serde_json::json!({"ok": true, "status": status})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

use axum::{Json, extract::State};

use crate::app_manager::AppManager;

pub(crate) async fn handler(State(manager): State<AppManager>) -> Json<serde_json::Value> {
    let app_names = manager.list().await;
    Json(serde_json::json!({"apps": app_names}))
}

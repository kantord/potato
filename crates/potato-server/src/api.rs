use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};

use crate::app_manager::{AppManager, RunningApp};
use crate::container;

#[derive(serde::Deserialize)]
struct ActivateRequest {
    image: String,
}

async fn activate_handler(
    State(manager): State<AppManager>,
    Json(body): Json<ActivateRequest>,
) -> Json<serde_json::Value> {
    let image = &body.image;

    if manager.contains(image).await {
        return Json(serde_json::json!({"ok": true, "status": "already_active"}));
    }

    let static_dir = match container::extract_image(image).await {
        Ok(dir) => dir,
        Err(e) => {
            return Json(
                serde_json::json!({"ok": false, "error": format!("failed to extract image: {e}")}),
            );
        }
    };

    let container_id = container::start_container(image).await.ok();

    let path = format!("/tmp/potato-{image}.sock");
    let _ = std::fs::remove_file(&path);

    let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            return Json(
                serde_json::json!({"ok": false, "error": format!("failed to bind socket: {e}")}),
            );
        }
    };

    let router = crate::app(static_dir, container_id.clone());
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    manager
        .insert(image.to_string(), RunningApp { container_id })
        .await;

    Json(serde_json::json!({"ok": true, "status": "activated"}))
}

async fn list_apps_handler(State(manager): State<AppManager>) -> Json<serde_json::Value> {
    let names = manager.list().await;
    Json(serde_json::json!({"apps": names}))
}

pub fn management_app(manager: AppManager) -> Router<()> {
    Router::new()
        .route("/activate", post(activate_handler))
        .route("/apps", get(list_apps_handler))
        .with_state(manager)
}

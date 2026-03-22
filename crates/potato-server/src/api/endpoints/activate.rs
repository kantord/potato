use axum::{Json, extract::State};

use crate::app_manager::{AppManager, RunningApp};
use crate::container::{AppContainer, extract_image};

#[derive(serde::Deserialize)]
pub(crate) struct ActivateRequest {
    image: String,
}

pub(crate) async fn handler(
    State(manager): State<AppManager>,
    Json(body): Json<ActivateRequest>,
) -> Json<serde_json::Value> {
    let image = &body.image;

    if manager.contains(image).await {
        return Json(serde_json::json!({"ok": true, "status": "already_active"}));
    }

    let static_dir = match extract_image(image).await {
        Ok(dir) => dir,
        Err(e) => {
            return Json(
                serde_json::json!({"ok": false, "error": format!("failed to extract image: {e}")}),
            );
        }
    };

    let container = AppContainer::start(image).await.ok();
    let container_id = container.as_ref().map(|c| c.id.clone());

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

    let router = crate::app(static_dir, container_id);
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    manager
        .insert(image.to_string(), RunningApp { container })
        .await;

    Json(serde_json::json!({"ok": true, "status": "activated"}))
}

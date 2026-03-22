use axum::extract::Path;
use axum::{Json, extract::State};
use tokio::io::AsyncWriteExt;

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

    for _ in 0..20 {
        {
            let writers = state.stdin_writers.lock().await;
            if let Some(writer) = writers.get(&call_id) {
                let mut guard = writer.lock().await;
                if let Some(ref mut w) = *guard
                    && w.write_all(line.as_bytes()).await.is_ok()
                {
                    return Json(serde_json::json!({"ok": true}));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    Json(serde_json::json!({"ok": false, "error": "call not found or not started"}))
}

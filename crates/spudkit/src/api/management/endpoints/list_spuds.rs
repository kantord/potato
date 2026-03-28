use axum::Json;

use crate::container::SpudkitImage;

pub(crate) async fn handler() -> Json<serde_json::Value> {
    match SpudkitImage::list_available().await {
        Ok(spuds) => {
            let names: Vec<&str> = spuds.iter().map(|s| s.name()).collect();
            Json(serde_json::json!({"spuds": names}))
        }
        Err(e) => Json(serde_json::json!({"spuds": [], "error": e.to_string()})),
    }
}

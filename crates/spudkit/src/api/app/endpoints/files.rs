use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::super::state::AppState;

pub(crate) async fn handler(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    let container = state.container.clone();
    let container_path = match crate::utils::resolve_container_path("/app/gui", &path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    match container.cat_file(&container_path).await {
        Ok(Some(bytes)) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();
            ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("exec failed: {e}"),
        )
            .into_response(),
    }
}

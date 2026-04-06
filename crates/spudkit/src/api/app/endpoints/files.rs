use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use super::super::state::AppState;

const POLYFILL: &str = include_str!("../../../polyfill.js");

fn inject_polyfill(html: &[u8]) -> Vec<u8> {
    let html_str = String::from_utf8_lossy(html);
    let script = format!("<script>{POLYFILL}</script>");
    if let Some(pos) = html_str.to_lowercase().rfind("</body>") {
        format!("{}{}{}", &html_str[..pos], script, &html_str[pos..]).into_bytes()
    } else {
        format!("{html_str}{script}").into_bytes()
    }
}

async fn serve_file(state: &AppState, path: &str) -> Response {
    let container = state.container.clone();
    let container_path = match crate::utils::resolve_container_path("/app/gui", path) {
        Some(p) => p,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    match container.cat_file(&container_path).await {
        Ok(Some(bytes)) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let body = if mime.starts_with("text/html") {
                inject_polyfill(&bytes)
            } else {
                bytes
            };
            ([(axum::http::header::CONTENT_TYPE, mime)], body).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("exec failed: {e}"),
        )
            .into_response(),
    }
}

pub(crate) async fn handler(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    serve_file(&state, &path).await
}

pub(crate) async fn fallback(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve_file(&state, "index.html").await;
    }
    serve_file(&state, path).await
}

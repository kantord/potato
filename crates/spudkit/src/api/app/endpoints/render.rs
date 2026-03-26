use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

use std::sync::LazyLock;

use super::super::state::AppState;
use crate::container::AppContainer;

static TEMPLATE_ENGINE: LazyLock<minijinja::Environment<'static>> =
    LazyLock::new(minijinja::Environment::new);

/// Parse stdin data from either JSON or form-encoded body.
fn parse_stdin_data(headers: &HeaderMap, body: &[u8]) -> Option<serde_json::Value> {
    if body.is_empty() {
        return None;
    }

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("application/x-www-form-urlencoded") {
        let pairs: Vec<(String, String)> = form_urlencoded::parse(body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let mut map = serde_json::Map::new();
        for (key, value) in pairs {
            map.insert(key, serde_json::Value::String(value));
        }
        Some(serde_json::Value::Object(map))
    } else {
        let parsed: serde_json::Value = serde_json::from_slice(body).ok()?;
        if let Some(data) = parsed.get("data") {
            Some(data.clone())
        } else {
            Some(parsed)
        }
    }
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    Path(script): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let container = AppContainer {
        id: state.container_id.clone(),
    };
    let resolved_cmd = crate::utils::resolve_cmd(std::slice::from_ref(&script));
    let stdin_data = parse_stdin_data(&headers, &body);

    let output_lines = match container.run(resolved_cmd, stdin_data.as_ref()).await {
        Ok(lines) => lines,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to exec: {e}"),
            )
                .into_response();
        }
    };

    let template_name = format!("{}.html", script.trim_start_matches('/'));

    let template_file = state.static_dir.join("app/templates").join(&template_name);

    let template_content = match std::fs::read_to_string(&template_file) {
        Ok(t) => t,
        Err(_) => {
            return output_lines.join("\n").into_response();
        }
    };

    // Parse output as JSON for template context
    let output_data: serde_json::Value = if output_lines.len() == 1 {
        serde_json::from_str(&output_lines[0]).unwrap_or(serde_json::json!(output_lines[0]))
    } else {
        let items: Vec<serde_json::Value> = output_lines
            .iter()
            .map(|line| serde_json::from_str(line).unwrap_or(serde_json::json!(line)))
            .collect();
        serde_json::json!({ "lines": items })
    };

    match TEMPLATE_ENGINE.render_str(&template_content, &output_data) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("template error: {e}"),
        )
            .into_response(),
    }
}

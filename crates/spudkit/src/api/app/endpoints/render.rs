use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

use std::sync::LazyLock;

use super::super::state::AppState;

static TEMPLATE_ENGINE: LazyLock<minijinja::Environment<'static>> = LazyLock::new(|| {
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
    env
});

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
    let container = state.container.clone();
    if crate::utils::resolve_container_path("/app/bin", &script).is_none() {
        return (axum::http::StatusCode::BAD_REQUEST, "invalid script name").into_response();
    }
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
    let template_path = match crate::utils::resolve_container_path("/app/templates", &template_name)
    {
        Some(p) => p,
        None => return output_lines.join("\n").into_response(),
    };

    let template_content = match container.cat_file(&template_path).await {
        Ok(Some(bytes)) => match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "template is not valid UTF-8",
                )
                    .into_response();
            }
        },
        _ => {
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

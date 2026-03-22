use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use bollard::container::LogOutput;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use super::super::state::AppState;
use crate::container::AppContainer;

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
        // Parse form fields into a JSON object
        let pairs: Vec<(String, String)> = form_urlencoded::parse(body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let mut map = serde_json::Map::new();
        for (key, value) in pairs {
            map.insert(key, serde_json::Value::String(value));
        }
        Some(serde_json::Value::Object(map))
    } else {
        // Try JSON — look for {"data": ...} wrapper or use as-is
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
    let container_id = match &state.container_id {
        Some(id) => id.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "no container available",
            )
                .into_response();
        }
    };

    let container = AppContainer { id: container_id };
    let resolved_cmd = crate::utils::resolve_cmd(std::slice::from_ref(&script));
    let attached = match container.exec(resolved_cmd).await {
        Ok(a) => a,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to exec: {e}"),
            )
                .into_response();
        }
    };

    // Send stdin if provided
    let stdin_data = parse_stdin_data(&headers, &body);
    let mut input = attached.input;
    if let Some(data) = &stdin_data {
        let line = serde_json::to_string(data).unwrap() + "\n";
        let _ = input.write_all(line.as_bytes()).await;
    }
    let _ = input.shutdown().await;
    drop(input);

    // Collect output
    let mut output_lines: Vec<String> = Vec::new();
    let mut output = attached.output;
    while let Some(Ok(log)) = output.next().await {
        let text = match &log {
            LogOutput::StdOut { message } => String::from_utf8_lossy(message).to_string(),
            _ => continue,
        };
        for line in text.lines() {
            if !line.is_empty() {
                output_lines.push(line.to_string());
            }
        }
    }

    // Look for a template matching the script name in /app/templates/
    let template_name = script
        .trim_start_matches('/')
        .strip_suffix(".sh")
        .unwrap_or(script.trim_start_matches('/'))
        .to_string()
        + ".html";

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

    let env = minijinja::Environment::new();
    match env.render_str(&template_content, &output_data) {
        Ok(html) => Html(html).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("template error: {e}"),
        )
            .into_response(),
    }
}

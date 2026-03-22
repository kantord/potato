use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use bollard::container::LogOutput;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use super::super::state::AppState;
use crate::container::AppContainer;

#[derive(serde::Deserialize)]
pub(crate) struct RenderRequest {
    cmd: Vec<String>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RenderRequest>,
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
    let attached = match container.exec(body.cmd.clone()).await {
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
    let mut input = attached.input;
    if let Some(data) = &body.data {
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

    let wants_html = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));

    if !wants_html {
        // Return raw JSON output
        let json_output: Vec<serde_json::Value> = output_lines
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        return Json(json_output).into_response();
    }

    // Look for a template matching the script name
    let script_name = body.cmd.first().map(|s| s.as_str()).unwrap_or("");
    let template_path = script_name
        .strip_suffix(".sh")
        .unwrap_or(script_name)
        .to_string()
        + ".html";

    let static_dir = &state.static_dir;
    let template_file = static_dir.join(template_path.trim_start_matches('/'));

    let template_content = match std::fs::read_to_string(&template_file) {
        Ok(t) => t,
        Err(_) => {
            // No template found — return plain text
            return output_lines.join("\n").into_response();
        }
    };

    // Parse output as JSON for template context
    let output_data: serde_json::Value = if output_lines.len() == 1 {
        serde_json::from_str(&output_lines[0]).unwrap_or(serde_json::json!(output_lines[0]))
    } else {
        // Try to parse each line as JSON, fall back to strings
        let items: Vec<serde_json::Value> = output_lines
            .iter()
            .map(|line| serde_json::from_str(line).unwrap_or(serde_json::json!(line)))
            .collect();
        serde_json::json!({ "lines": items })
    };

    // Render template
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

use axum::Json;
use axum::extract::State;
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
    let resolved_cmd = crate::utils::resolve_cmd(&body.cmd);
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

    // Look for a template matching the script name in /app/templates/
    let script_name = body.cmd.first().map(|s| s.as_str()).unwrap_or("");
    let template_name = script_name
        .trim_start_matches('/')
        .strip_suffix(".sh")
        .unwrap_or(script_name.trim_start_matches('/'))
        .to_string()
        + ".html";

    let template_file = state.static_dir.join("app/templates").join(&template_name);

    let template_content = match std::fs::read_to_string(&template_file) {
        Ok(t) => t,
        Err(_) => {
            // No template — return plain text
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

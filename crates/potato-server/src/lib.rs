use axum::response::sse::{Event, Sse};
use axum::{Json, Router, extract::State, routing::post};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::{Stream, StreamExt};
use std::convert::Infallible;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    container_id: String,
}

#[derive(serde::Deserialize)]
struct RunRequest {
    cmd: Vec<String>,
    #[serde(default)]
    stdin: Option<serde_json::Value>,
}

fn tag_line(line: &str, default_event: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
        if parsed.get("event").is_some() {
            // Already tagged, pass through
            return serde_json::to_string(&parsed).unwrap_or_else(|_| line.to_string());
        }
        // JSON without event tag — wrap it
        let tagged = serde_json::json!({
            "event": default_event,
            "data": parsed,
        });
        serde_json::to_string(&tagged).unwrap()
    } else {
        // Plain text — wrap it
        let tagged = serde_json::json!({
            "event": default_event,
            "data": line,
        });
        serde_json::to_string(&tagged).unwrap()
    }
}

async fn run_command(
    State(state): State<AppState>,
    Json(body): Json<RunRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(e) => {
                let msg = tag_line(&format!("failed to connect to docker: {e}"), "error");
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

        let exec = match docker
            .create_exec(
                &state.container_id,
                CreateExecOptions {
                    cmd: Some(body.cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(body.stdin.is_some()),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(e) => e,
            Err(e) => {
                let msg = tag_line(&format!("failed to create exec: {e}"), "error");
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

        match docker.start_exec(&exec.id, None).await {
            Ok(StartExecResults::Attached { mut output, mut input }) => {
                // Write stdin if provided
                if let Some(stdin_data) = body.stdin {
                    let stdin_str = serde_json::to_string(&stdin_data).unwrap() + "\n";
                    let _ = input.write_all(stdin_str.as_bytes()).await;
                    let _ = input.shutdown().await;
                }
                drop(input);

                while let Some(Ok(log)) = output.next().await {
                    let (text, default_event) = match &log {
                        LogOutput::StdOut { message } => {
                            (String::from_utf8_lossy(message).to_string(), "output")
                        }
                        LogOutput::StdErr { message } => {
                            (String::from_utf8_lossy(message).to_string(), "error")
                        }
                        _ => continue,
                    };

                    for line in text.lines() {
                        if line.is_empty() {
                            continue;
                        }
                        let tagged = tag_line(line, default_event);
                        if tx.send(Ok(Event::default().data(tagged))).await.is_err() {
                            return;
                        }
                    }
                }
            }
            Ok(StartExecResults::Detached) => {}
            Err(e) => {
                let msg = tag_line(&format!("failed to start exec: {e}"), "error");
                let _ = tx.send(Ok(Event::default().data(msg))).await;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
}

pub async fn start_container(image: &str) -> Result<String, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let name = format!("potato-{}", uuid());
    let container = docker
        .create_container(
            Some(CreateContainerOptions { name: Some(name), ..Default::default() }),
            config,
        )
        .await?;

    docker.start_container(&container.id, None).await?;

    Ok(container.id)
}

pub async fn stop_container(container_id: &str) {
    if let Ok(docker) = Docker::connect_with_local_defaults() {
        let _ = docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions { force: true, ..Default::default() }),
            )
            .await;
    }
}

pub async fn extract_image(image: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["true".to_string()]),
        ..Default::default()
    };

    let name = format!("potato-extract-{}", uuid());
    let container = docker
        .create_container(
            Some(CreateContainerOptions { name: Some(name), ..Default::default() }),
            config,
        )
        .await?;

    let mut tar_stream = docker.export_container(&container.id);
    let mut tar_bytes = Vec::new();
    while let Some(chunk) = tar_stream.next().await {
        tar_bytes.extend_from_slice(&chunk?);
    }

    let _ = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions { force: true, ..Default::default() }),
        )
        .await;

    let extract_dir = std::env::temp_dir().join(format!("potato-{}", uuid()));
    std::fs::create_dir_all(&extract_dir)?;

    let mut archive = tar::Archive::new(&tar_bytes[..]);
    archive.unpack(&extract_dir)?;

    Ok(extract_dir)
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{n:x}")
}

pub fn app(static_dir: PathBuf, container_id: String) -> Router {
    let state = AppState { container_id };
    Router::new()
        .route("/run", post(run_command))
        .nest_service("/files", ServeDir::new(static_dir))
        .with_state(state)
}

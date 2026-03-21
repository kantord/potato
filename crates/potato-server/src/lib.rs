use axum::extract::Path;
use axum::response::sse::{Event, Sse};
use axum::{Json, Router, extract::State, routing::{get, post}};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::{Stream, StreamExt};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tower_http::services::ServeDir;

type StdinWriter = Arc<Mutex<Option<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>>;

struct PendingCall {
    cmd: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    container_id: Option<String>,
    pending: Arc<Mutex<HashMap<String, PendingCall>>>,
    stdin_writers: Arc<Mutex<HashMap<String, StdinWriter>>>,
}

#[derive(serde::Deserialize)]
struct CreateCallRequest {
    cmd: Vec<String>,
}

#[derive(serde::Serialize)]
struct CreateCallResponse {
    call_id: String,
}

#[derive(serde::Deserialize)]
struct StdinRequest {
    data: serde_json::Value,
}

fn tag_line(line: &str, default_event: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
        if parsed.get("event").is_some() {
            return serde_json::to_string(&parsed).unwrap_or_else(|_| line.to_string());
        }
        let tagged = serde_json::json!({
            "event": default_event,
            "data": parsed,
        });
        serde_json::to_string(&tagged).unwrap()
    } else {
        let tagged = serde_json::json!({
            "event": default_event,
            "data": line,
        });
        serde_json::to_string(&tagged).unwrap()
    }
}

// POST /calls — register a call (does NOT start the process)
async fn create_call(
    State(state): State<AppState>,
    Json(body): Json<CreateCallRequest>,
) -> Json<CreateCallResponse> {
    let call_id = uuid();
    state.pending.lock().await.insert(
        call_id.clone(),
        PendingCall { cmd: body.cmd },
    );
    Json(CreateCallResponse { call_id })
}

// GET /calls/{id}/events — start the exec and stream output
async fn call_events(
    State(state): State<AppState>,
    Path(call_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    let pending_call = state.pending.lock().await.remove(&call_id);

    let container_id = state.container_id.clone();
    let stdin_writers = state.stdin_writers.clone();
    let cid = call_id.clone();

    tokio::spawn(async move {
        let call = match pending_call {
            Some(c) => c,
            None => {
                let msg = tag_line("call not found or already started", "error");
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

        let container_id = match container_id {
            Some(id) => id,
            None => {
                let msg = tag_line("no container available for this app", "error");
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

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
                &container_id,
                CreateExecOptions {
                    cmd: Some(call.cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
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
            Ok(StartExecResults::Attached { mut output, input }) => {
                let stdin_writer: StdinWriter = Arc::new(Mutex::new(Some(Box::new(input))));
                stdin_writers.lock().await.insert(cid.clone(), stdin_writer);

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
                            break;
                        }
                    }
                }

                let end_msg = serde_json::json!({"event":"end"}).to_string();
                let _ = tx.send(Ok(Event::default().data(end_msg))).await;
                stdin_writers.lock().await.remove(&cid);
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

// POST /calls/{id}/stdin — send input to the call
async fn call_stdin(
    State(state): State<AppState>,
    Path(call_id): Path<String>,
    Json(body): Json<StdinRequest>,
) -> Json<serde_json::Value> {
    let writers = state.stdin_writers.lock().await;
    if let Some(writer) = writers.get(&call_id) {
        let mut guard = writer.lock().await;
        if let Some(ref mut w) = *guard {
            let line = serde_json::to_string(&body.data).unwrap() + "\n";
            if w.write_all(line.as_bytes()).await.is_ok() {
                return Json(serde_json::json!({"ok": true}));
            }
        }
    }
    Json(serde_json::json!({"ok": false, "error": "call not found or not started"}))
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

    let extract_dir = std::env::temp_dir().join(format!("potato-{}", uuid()));
    std::fs::create_dir_all(&extract_dir)?;

    // Stream tar directly to disk via a pipe — avoids loading entire image into memory
    let (pipe_reader, mut pipe_writer) = os_pipe::pipe()?;
    let extract_dir_clone = extract_dir.clone();

    let unpack_handle = std::thread::spawn(move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut archive = tar::Archive::new(pipe_reader);
        archive.unpack(&extract_dir_clone)?;
        Ok(())
    });

    let mut tar_stream = docker.export_container(&container.id);
    while let Some(chunk) = tar_stream.next().await {
        let chunk = chunk?;
        std::io::Write::write_all(&mut pipe_writer, &chunk)?;
    }
    drop(pipe_writer);

    unpack_handle.join().map_err(|_| "unpack thread panicked")?
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(std::io::Error::other(e.to_string())) })?;

    let _ = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions { force: true, ..Default::default() }),
        )
        .await;

    Ok(extract_dir)
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn app(static_dir: PathBuf, container_id: Option<String>) -> Router {
    let state = AppState {
        container_id,
        pending: Arc::new(Mutex::new(HashMap::new())),
        stdin_writers: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/calls", post(create_call))
        .route("/calls/{id}/events", get(call_events))
        .route("/calls/{id}/stdin", post(call_stdin))
        .nest_service("/files", ServeDir::new(static_dir))
        .with_state(state)
}

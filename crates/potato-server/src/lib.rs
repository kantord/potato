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
type CallRegistry = Arc<Mutex<HashMap<String, StdinWriter>>>;

#[derive(Clone)]
struct AppState {
    container_id: Option<String>,
    calls: CallRegistry,
}

#[derive(serde::Deserialize)]
struct RunRequest {
    cmd: Vec<String>,
    #[serde(default)]
    stdin: Option<serde_json::Value>,
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

// Legacy /run endpoint (single request/response, stdin sent once)
async fn run_command(
    State(state): State<AppState>,
    Json(body): Json<RunRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let container_id = match &state.container_id {
            Some(id) => id.clone(),
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
                if let Some(stdin_data) = body.stdin {
                    let stdin_str = serde_json::to_string(&stdin_data).unwrap() + "\n";
                    let _ = input.write_all(stdin_str.as_bytes()).await;
                    let _ = input.shutdown().await;
                }
                drop(input);

                stream_output(&mut output, &tx).await;
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

// POST /calls — start a new call
async fn create_call(
    State(state): State<AppState>,
    Json(body): Json<CreateCallRequest>,
) -> Json<CreateCallResponse> {
    let call_id = uuid();

    let container_id = match &state.container_id {
        Some(id) => id.clone(),
        None => {
            return Json(CreateCallResponse { call_id });
        }
    };

    let calls = state.calls.clone();
    let cid = call_id.clone();

    tokio::spawn(async move {
        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(_) => return,
        };

        let exec = match docker
            .create_exec(
                &container_id,
                CreateExecOptions {
                    cmd: Some(body.cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(e) => e,
            Err(_) => return,
        };

        if let Ok(StartExecResults::Attached { output, input }) = docker.start_exec(&exec.id, None).await {
            let stdin_writer: StdinWriter = Arc::new(Mutex::new(Some(Box::new(input))));
            calls.lock().await.insert(cid.clone(), stdin_writer);

            // Store output sender — we'll set it up when events are requested
            // For now, store the output in a broadcast channel
            let (tx, _rx) = tokio::sync::broadcast::channel::<String>(256);
            let tx = Arc::new(tx);

            // Store the broadcast sender alongside the stdin writer
            // We need a second registry for output senders
            OUTPUT_REGISTRY.lock().await.insert(cid.clone(), tx.clone());

            let mut output = output;
            let cid_cleanup = cid.clone();
            let calls_cleanup = calls.clone();

            // Stream output to broadcast
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
                    let _ = tx.send(tagged);
                }
            }

            // Process ended — send end event and clean up
            let _ = tx.send(serde_json::json!({"event":"end"}).to_string());
            calls_cleanup.lock().await.remove(&cid_cleanup);
            OUTPUT_REGISTRY.lock().await.remove(&cid_cleanup);
        }
    });

    // Small delay to let the exec start and register
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    Json(CreateCallResponse { call_id })
}

// GET /calls/{id}/events — SSE stream of output
async fn call_events(
    Path(call_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let broadcast_tx = {
            let registry = OUTPUT_REGISTRY.lock().await;
            registry.get(&call_id).cloned()
        };

        if let Some(broadcast_tx) = broadcast_tx {
            let mut broadcast_rx = broadcast_tx.subscribe();
            while let Ok(msg) = broadcast_rx.recv().await {
                if tx.send(Ok(Event::default().data(msg))).await.is_err() {
                    break;
                }
            }
        } else {
            let msg = tag_line("call not found", "error");
            let _ = tx.send(Ok(Event::default().data(msg))).await;
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
    let calls = state.calls.lock().await;
    if let Some(writer) = calls.get(&call_id) {
        let mut guard = writer.lock().await;
        if let Some(ref mut w) = *guard {
            let line = serde_json::to_string(&body.data).unwrap() + "\n";
            if w.write_all(line.as_bytes()).await.is_ok() {
                return Json(serde_json::json!({"ok": true}));
            }
        }
    }
    Json(serde_json::json!({"ok": false, "error": "call not found or stdin closed"}))
}

async fn stream_output(
    output: &mut (impl Stream<Item = Result<LogOutput, bollard::errors::Error>> + Unpin),
    tx: &mpsc::Sender<Result<Event, Infallible>>,
) {
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

use tokio::sync::broadcast;
use std::sync::LazyLock;

type OutputRegistry = Arc<Mutex<HashMap<String, Arc<broadcast::Sender<String>>>>>;

static OUTPUT_REGISTRY: LazyLock<OutputRegistry> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

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

pub fn app(static_dir: PathBuf, container_id: Option<String>) -> Router {
    let state = AppState {
        container_id,
        calls: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route("/run", post(run_command))
        .route("/calls", post(create_call))
        .route("/calls/{id}/events", get(call_events))
        .route("/calls/{id}/stdin", post(call_stdin))
        .nest_service("/files", ServeDir::new(static_dir))
        .with_state(state)
}

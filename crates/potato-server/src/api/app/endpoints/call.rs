use axum::response::sse::{Event, Sse};
use axum::{Json, extract::State};
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::{Stream, StreamExt};
use potato_transport::SseEvent;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::super::state::{AppState, StdinWriter};

#[derive(serde::Deserialize)]
pub(crate) struct CreateCallRequest {
    cmd: Vec<String>,
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    Json(body): Json<CreateCallRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    let call_id = crate::uuid();
    let container_id = state.container_id.clone();
    let stdin_writers = state.stdin_writers.clone();
    let cid = call_id.clone();

    tokio::spawn(async move {
        let container_id = match container_id {
            Some(id) => id,
            None => {
                let msg = SseEvent::Error(serde_json::json!("no container available for this app"))
                    .to_json();
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

        let docker = match Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(e) => {
                let msg = SseEvent::Error(serde_json::json!(format!(
                    "failed to connect to docker: {e}"
                )))
                .to_json();
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
                    attach_stdin: Some(true),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(e) => e,
            Err(e) => {
                let msg = SseEvent::Error(serde_json::json!(format!("failed to create exec: {e}")))
                    .to_json();
                let _ = tx.send(Ok(Event::default().data(msg))).await;
                return;
            }
        };

        match docker.start_exec(&exec.id, None).await {
            Ok(StartExecResults::Attached { mut output, input }) => {
                let stdin_writer: StdinWriter = Arc::new(Mutex::new(Some(Box::new(input))));
                stdin_writers.lock().await.insert(cid.clone(), stdin_writer);

                let _ = tx
                    .send(Ok(Event::default().data(
                        SseEvent::Started {
                            call_id: cid.clone(),
                        }
                        .to_json(),
                    )))
                    .await;

                while let Some(Ok(log)) = output.next().await {
                    let (text, is_stderr) = match &log {
                        LogOutput::StdOut { message } => {
                            (String::from_utf8_lossy(message).to_string(), false)
                        }
                        LogOutput::StdErr { message } => {
                            (String::from_utf8_lossy(message).to_string(), true)
                        }
                        _ => continue,
                    };

                    for line in text.lines() {
                        if line.is_empty() {
                            continue;
                        }
                        let event = if is_stderr {
                            SseEvent::from_stderr(line)
                        } else {
                            SseEvent::from_stdout(line)
                        };
                        if tx
                            .send(Ok(Event::default().data(event.to_json())))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }

                let _ = tx
                    .send(Ok(Event::default().data(SseEvent::End.to_json())))
                    .await;
                stdin_writers.lock().await.remove(&cid);
            }
            Ok(StartExecResults::Detached) => {}
            Err(e) => {
                let msg = SseEvent::Error(serde_json::json!(format!("failed to start exec: {e}")))
                    .to_json();
                let _ = tx.send(Ok(Event::default().data(msg))).await;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
}

use axum::response::sse::{Event, Sse};
use axum::{Json, extract::State};
use bollard::container::LogOutput;
use futures_util::{Stream, StreamExt};
use spudkit_core::SseEvent;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::super::state::{AppState, StdinWriter};
use crate::sse_stream::SseStream;

#[derive(serde::Deserialize)]
pub(crate) struct CreateCallRequest {
    cmd: Vec<String>,
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    Json(body): Json<CreateCallRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (stream, sse) = SseStream::create();

    let call_id = crate::utils::generate_id();
    let container = state.container.clone();
    let stdin_writers = state.stdin_writers.clone();
    let cid = call_id.clone();

    tokio::spawn(async move {
        let resolved_cmd = crate::utils::resolve_cmd(&body.cmd);
        let attached = match container.call(&resolved_cmd).await {
            Ok(a) => a,
            Err(e) => return stream.error(&format!("failed to exec: {e}")).await,
        };

        let stdin_writer: StdinWriter = Arc::new(Mutex::new(Some(attached.input)));
        stdin_writers.lock().await.insert(cid.clone(), stdin_writer);

        let _ = stream
            .send(SseEvent::Started {
                call_id: cid.clone(),
            })
            .await;

        let mut output = attached.output;
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
                if !stream.send(event).await {
                    break;
                }
            }
        }

        let _ = stream.send(SseEvent::End).await;
        stdin_writers.lock().await.remove(&cid);
    });

    sse
}

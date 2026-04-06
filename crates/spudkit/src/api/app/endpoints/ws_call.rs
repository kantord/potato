use axum::{
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use bollard::container::LogOutput;
use futures_util::{SinkExt, StreamExt};
use spudkit_core::SseEvent;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use super::super::state::AppState;

pub(crate) async fn handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(ws: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = ws.split();

    // First message must be {"cmd": [...]}
    let cmd = match receiver.next().await {
        Some(Ok(Message::Text(text))) => {
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
                return;
            };
            let Some(arr) = parsed["cmd"].as_array() else {
                return;
            };
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        }
        _ => return,
    };

    let resolved_cmd = crate::utils::resolve_cmd(&cmd);
    let attached = match state.container.call(&resolved_cmd).await {
        Ok(a) => a,
        Err(e) => {
            let msg = SseEvent::Error(serde_json::json!(format!("failed to exec: {e}"))).to_json();
            let _ = sender.send(Message::Text(msg.into())).await;
            return;
        }
    };

    let call_id = crate::utils::generate_id();
    let _ = sender
        .send(Message::Text(
            SseEvent::Started { call_id }.to_json().into(),
        ))
        .await;

    // Channel that serialises output events to the WS sender task.
    let (out_tx, mut out_rx) = mpsc::channel::<String>(64);

    // Read process output, push JSON event strings into the channel.
    let mut output = attached.output;
    tokio::spawn(async move {
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
                if out_tx.send(event.to_json()).await.is_err() {
                    return;
                }
            }
        }
        let _ = out_tx.send(SseEvent::End.to_json()).await;
    });

    // Forward output channel → WS sender. Close the WS when the process finishes.
    tokio::spawn(async move {
        while let Some(json) = out_rx.recv().await {
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
        let _ = sender.close().await;
    });

    // Receive stdin messages from the client and write to the process.
    let mut input = attached.input;
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                if parsed.get("type").and_then(|v| v.as_str()) == Some("stdin") {
                    if let Some(data) = parsed.get("data") {
                        let line = serde_json::to_string(data).unwrap() + "\n";
                        if input.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

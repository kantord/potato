use axum::{
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use futures_util::StreamExt;

use super::super::state::AppState;

pub(crate) async fn handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(ws: WebSocket, state: AppState) {
    let (_, mut receiver) = ws.split();

    // Each message: {"call_id": "...", "data": ...}
    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(call_id) = parsed["call_id"].as_str() else {
            continue;
        };
        if let Some(data) = parsed.get("data") {
            let line = serde_json::to_string(data).unwrap() + "\n";
            state.write_stdin(call_id, line.as_bytes()).await;
        }
    }
}

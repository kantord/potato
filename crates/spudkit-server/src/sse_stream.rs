use axum::response::sse::{Event, Sse};
use futures_util::Stream;
use spudkit_transport::SseEvent;
use std::convert::Infallible;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// A sender for SSE events to a connected client.
pub(crate) struct SseStream {
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl SseStream {
    /// Create a new SSE stream. Returns the sender and the axum SSE response.
    pub fn create() -> (Self, Sse<impl Stream<Item = Result<Event, Infallible>>>) {
        let (tx, rx) = mpsc::channel(32);
        (Self { tx }, Sse::new(ReceiverStream::new(rx)))
    }

    /// Send an event to the client. Returns false if the client disconnected.
    pub async fn send(&self, event: SseEvent) -> bool {
        self.tx
            .send(Ok(Event::default().data(event.to_json())))
            .await
            .is_ok()
    }

    /// Send an error message to the client.
    pub async fn error(&self, msg: &str) {
        let _ = self.send(SseEvent::Error(serde_json::json!(msg))).await;
    }
}

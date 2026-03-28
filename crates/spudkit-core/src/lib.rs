mod spud;
pub use spud::Spud;

use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

/// A tagged event in the spudkit protocol.
/// Used on both the server side (creating events from container output)
/// and the client side (parsing events from SSE streams).
pub enum SseEvent {
    Started {
        call_id: String,
    },
    Output(serde_json::Value),
    Error(serde_json::Value),
    Custom {
        event: String,
        data: serde_json::Value,
    },
    End,
}

impl SseEvent {
    /// Create an event from a raw stdout line.
    /// If the line is already tagged JSON (has an "event" field), it is parsed as-is.
    /// Otherwise, it is wrapped as an Output event.
    pub fn from_stdout(line: &str) -> Self {
        Self::from_line(line, "output")
    }

    /// Create an event from a raw stderr line.
    /// If the line is already tagged JSON (has an "event" field), it is parsed as-is.
    /// Otherwise, it is wrapped as an Error event.
    pub fn from_stderr(line: &str) -> Self {
        Self::from_line(line, "error")
    }

    fn from_line(line: &str, default_event: &str) -> Self {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(event) = parsed.get("event").and_then(|e| e.as_str()) {
                let data = parsed.get("data").cloned().unwrap_or(parsed.clone());
                return match event {
                    "started" => {
                        let call_id = parsed["data"]["call_id"].as_str().unwrap_or("").to_string();
                        Self::Started { call_id }
                    }
                    "end" => Self::End,
                    "error" => Self::Error(data),
                    "output" => Self::Output(data),
                    _ => Self::Custom {
                        event: event.to_string(),
                        data,
                    },
                };
            }
            if default_event == "error" {
                return Self::Error(parsed);
            }
            return Self::Output(parsed);
        }
        let text = serde_json::Value::String(line.to_string());
        if default_event == "error" {
            Self::Error(text)
        } else {
            Self::Output(text)
        }
    }

    /// Format the event's data for human-readable display.
    /// Strings are returned unwrapped, everything else as JSON.
    pub fn display_data(&self) -> Option<String> {
        let data = match self {
            Self::Output(d) | Self::Error(d) | Self::Custom { data: d, .. } => d,
            Self::Started { .. } | Self::End => return None,
        };
        Some(match data {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Serialize the event to a JSON string suitable for SSE data.
    pub fn to_json(&self) -> String {
        let value = match self {
            Self::Started { call_id } => {
                serde_json::json!({"event": "started", "data": {"call_id": call_id}})
            }
            Self::Output(data) => serde_json::json!({"event": "output", "data": data}),
            Self::Error(data) => serde_json::json!({"event": "error", "data": data}),
            Self::Custom { event, data } => serde_json::json!({"event": event, "data": data}),
            Self::End => serde_json::json!({"event": "end"}),
        };
        value.to_string()
    }
}

/// A connection to a Unix socket endpoint.
#[derive(Clone)]
pub struct SpudkitConnection {
    path: String,
}

impl SpudkitConnection {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Send an HTTP request and return the raw streaming response.
    async fn request_raw(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        extra_headers: &[(&str, &str)],
    ) -> anyhow::Result<hyper::Response<hyper::body::Incoming>> {
        let client: Client<UnixConnector, Full<Bytes>> = Client::unix();
        let uri: hyper::Uri = Uri::new(&self.path, path).into();
        let method: Method = method.parse().context("invalid HTTP method")?;
        let body_bytes = body.unwrap_or(&[]);

        let has_content_type = extra_headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Host", "localhost");

        if !has_content_type {
            builder = builder.header("Content-Type", "application/json");
        }

        for (key, value) in extra_headers {
            builder = builder.header(*key, *value);
        }

        let request = builder
            .body(Full::new(Bytes::copy_from_slice(body_bytes)))
            .context("failed to build request")?;

        client.request(request).await.context("request failed")
    }

    /// Send an HTTP request and return the full response body.
    pub async fn fetch(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> anyhow::Result<Vec<u8>> {
        self.fetch_with_headers(method, path, body, &[]).await
    }

    /// Send an HTTP request with custom headers and return the full response body.
    pub async fn fetch_with_headers(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        headers: &[(&str, &str)],
    ) -> anyhow::Result<Vec<u8>> {
        let response = self.request_raw(method, path, body, headers).await?;

        let body = response
            .into_body()
            .collect()
            .await
            .context("failed to read response body")?
            .to_bytes();

        Ok(body.to_vec())
    }

    /// Stream raw SSE data lines. Calls `on_line` for each `data:` line
    /// (with prefix stripped). Sends `{"event":"end"}` when the stream closes.
    async fn stream_raw(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        mut on_line: impl FnMut(&str),
    ) -> anyhow::Result<()> {
        let response = self
            .request_raw(method, path, body, &[])
            .await
            .context("failed to connect")?;

        let mut body = response.into_body();
        let mut buffer = String::new();

        while let Some(result) = body.frame().await {
            match result {
                Ok(frame) => {
                    if let Some(data) = frame.data_ref() {
                        buffer.push_str(&String::from_utf8_lossy(data));
                        process_buffer(&mut buffer, &mut on_line);
                    }
                }
                Err(_) => break,
            }
        }

        on_line(r#"{"event":"end"}"#);

        Ok(())
    }

    /// Stream SSE events. Calls `on_event` for each parsed event.
    pub async fn stream(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        mut on_event: impl FnMut(SseEvent),
    ) -> anyhow::Result<()> {
        self.stream_raw(method, path, body, |data| {
            if let Some(event) = parse_sse_line(data) {
                on_event(event);
            }
        })
        .await
    }
}

fn parse_sse_line(data: &str) -> Option<SseEvent> {
    Some(SseEvent::from_stdout(data))
}

fn process_buffer(buffer: &mut String, on_line: &mut impl FnMut(&str)) {
    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].to_string();
        *buffer = buffer[pos + 1..].to_string();

        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if !data.is_empty() {
                on_line(data);
            }
        }
    }
}

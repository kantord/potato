use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

/// A parsed SSE event with its event type and data.
pub enum SseEvent {
    Started { call_id: String },
    Output(serde_json::Value),
    Error(serde_json::Value),
    End,
}

fn parse_sse_line(data: &str) -> Option<SseEvent> {
    let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
    let event = parsed
        .get("event")
        .and_then(|e| e.as_str())
        .unwrap_or("output");

    match event {
        "started" => {
            let call_id = parsed["data"]["call_id"].as_str().unwrap_or("").to_string();
            Some(SseEvent::Started { call_id })
        }
        "end" => Some(SseEvent::End),
        "error" => parsed.get("data").map(|d| SseEvent::Error(d.clone())),
        _ => parsed.get("data").map(|d| SseEvent::Output(d.clone())),
    }
}

async fn open_sse_response(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> anyhow::Result<hyper::Response<hyper::body::Incoming>> {
    let client: Client<UnixConnector, Full<Bytes>> = Client::unix();
    let uri: hyper::Uri = Uri::new(socket_path, path).into();
    let method: Method = method.parse()?;
    let body_bytes = body.unwrap_or(&[]);

    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::copy_from_slice(body_bytes)))?;

    let response = client.request(request).await?;
    Ok(response)
}

/// Stream raw SSE data lines from a Unix socket. Calls `on_line` for each `data:` line
/// (with prefix stripped). Sends `{"event":"end"}` when the stream closes.
pub async fn stream_sse_raw(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    mut on_line: impl FnMut(&str),
) {
    let response = match open_sse_response(socket_path, method, path, body).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to connect: {e}");
            std::process::exit(1);
        }
    };

    let mut body = response.into_body();
    let mut buffer = String::new();

    use http_body_util::BodyExt;
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

/// Stream SSE events from a Unix socket. Calls `on_event` for each parsed event.
/// Returns when the stream ends or the connection closes.
pub async fn stream_sse(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    mut on_event: impl FnMut(SseEvent),
) {
    stream_sse_raw(socket_path, method, path, body, |data| {
        if let Some(event) = parse_sse_line(data) {
            on_event(event);
        }
    })
    .await;
}

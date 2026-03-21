#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use tauri::ipc::Channel;
use tauri::Manager;
use tauri::webview::WebviewWindowBuilder;

const APPS: &[&str] = &["potato-hello-world", "potato-hello-simple"];

struct SocketPath(String);

fn forward_to_socket(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("failed to connect to socket: {e}"))?;

    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n"
    );
    if let Some(b) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    request.push_str("\r\n");

    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("failed to write: {e}"))?;
    if let Some(b) = body {
        stream
            .write_all(b)
            .map_err(|e| format!("failed to write body: {e}"))?;
    }

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("failed to read: {e}"))?;

    if let Some(pos) = String::from_utf8_lossy(&response).find("\r\n\r\n") {
        Ok(response[pos + 4..].to_vec())
    } else {
        Ok(response)
    }
}

fn read_sse_lines(socket_path: &str, method: &str, path: &str, body: Option<&[u8]>, on_event: &Channel<String>) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("failed to connect to socket: {e}"))?;

    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n"
    );
    if let Some(b) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).map_err(|e| format!("failed to write: {e}"))?;
    if let Some(b) = body {
        stream.write_all(b).map_err(|e| format!("failed to write body: {e}"))?;
    }

    let reader = BufReader::new(stream);
    let mut past_headers = false;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("failed to read: {e}"))?;

        if !past_headers {
            if line.is_empty() {
                past_headers = true;
            }
            continue;
        }

        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if !data.is_empty() {
                let _ = on_event.send(data.to_string());
            }
        }
    }

    let _ = on_event.send(r#"{"event":"end"}"#.to_string());
    Ok(())
}

#[tauri::command]
async fn stream_run(
    state: tauri::State<'_, SocketPath>,
    body: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    let socket_path = state.0.clone();

    tokio::task::spawn_blocking(move || {
        read_sse_lines(&socket_path, "POST", "/run", Some(body.as_bytes()), &on_event)
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?
}

#[tauri::command]
async fn create_call(
    state: tauri::State<'_, SocketPath>,
    body: String,
) -> Result<String, String> {
    let socket_path = state.0.clone();

    tokio::task::spawn_blocking(move || {
        let response = forward_to_socket(&socket_path, "POST", "/calls", Some(body.as_bytes()))?;
        String::from_utf8(response).map_err(|e| format!("invalid response: {e}"))
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?
}

#[tauri::command]
async fn stream_call_events(
    state: tauri::State<'_, SocketPath>,
    call_id: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    let socket_path = state.0.clone();
    let path = format!("/calls/{call_id}/events");

    tokio::task::spawn_blocking(move || {
        read_sse_lines(&socket_path, "GET", &path, None, &on_event)
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?
}

#[tauri::command]
async fn send_call_stdin(
    state: tauri::State<'_, SocketPath>,
    call_id: String,
    data: String,
) -> Result<String, String> {
    let socket_path = state.0.clone();
    let path = format!("/calls/{call_id}/stdin");

    tokio::task::spawn_blocking(move || {
        let response = forward_to_socket(&socket_path, "POST", &path, Some(data.as_bytes()))?;
        String::from_utf8(response).map_err(|e| format!("invalid response: {e}"))
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?
}

fn mime_for_path(path: &str) -> &'static str {
    if path.ends_with(".html") || path == "/index.html" {
        "text/html"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

fn main() {
    let app_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: potato-client <app-name>");
        eprintln!("Available apps: {}", APPS.join(", "));
        std::process::exit(1);
    });

    if !APPS.contains(&app_name.as_str()) {
        eprintln!("Unknown app: {app_name}");
        eprintln!("Available apps: {}", APPS.join(", "));
        std::process::exit(1);
    }

    let socket_path = format!("/tmp/potato-{app_name}.sock");
    let socket_path_for_protocol = socket_path.clone();

    tauri::Builder::default()
        .register_uri_scheme_protocol("potato", move |_ctx, request| {
            let mut path = request.uri().path().to_string();

            if path == "/" || path.is_empty() {
                path = "/index.html".to_string();
            }
            let server_path = format!("/files{path}");

            match forward_to_socket(&socket_path_for_protocol, "GET", &server_path, None) {
                Ok(response_body) => tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime_for_path(&path))
                    .body(response_body)
                    .unwrap(),
                Err(e) => tauri::http::Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(e.into_bytes())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![stream_run, create_call, stream_call_events, send_call_stdin])
        .setup(move |app| {
            app.manage(SocketPath(socket_path));

            let polyfill = include_str!("../frontend/potato-polyfill.js");

            let window = WebviewWindowBuilder::new(app, "main", Default::default())
                .title("Potato")
                .inner_size(800.0, 600.0)
                .initialization_script(polyfill)
                .build()?;

            window.navigate("potato://localhost".parse().unwrap())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

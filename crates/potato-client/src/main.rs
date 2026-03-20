#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use tauri::Manager;

fn forward_to_socket(method: &str, path: &str, body: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut stream = UnixStream::connect("/tmp/potato.sock")
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

    stream.write_all(request.as_bytes())
        .map_err(|e| format!("failed to write: {e}"))?;
    if let Some(b) = body {
        stream.write_all(b)
            .map_err(|e| format!("failed to write body: {e}"))?;
    }

    let mut response = Vec::new();
    stream.read_to_end(&mut response)
        .map_err(|e| format!("failed to read: {e}"))?;

    if let Some(pos) = String::from_utf8_lossy(&response).find("\r\n\r\n") {
        Ok(response[pos + 4..].to_vec())
    } else {
        Ok(response)
    }
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
    tauri::Builder::default()
        .register_uri_scheme_protocol("potato", |_ctx, request| {
            let method = request.method().as_str().to_string();
            let mut path = request.uri().path().to_string();

            let body_bytes = request.into_body();
            let body = if !body_bytes.is_empty() {
                Some(body_bytes)
            } else {
                None
            };

            // Route: /run goes directly, everything else is a file
            let server_path = if path.starts_with("/run") {
                path.clone()
            } else {
                if path == "/" || path.is_empty() {
                    path = "/index.html".to_string();
                }
                format!("/files{path}")
            };

            match forward_to_socket(&method, &server_path, body.as_deref()) {
                Ok(response_body) => {
                    let mime = if path.starts_with("/run") {
                        "text/event-stream"
                    } else {
                        mime_for_path(&path)
                    };

                    tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", mime)
                        .body(response_body)
                        .unwrap()
                }
                Err(e) => tauri::http::Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(e.into_bytes())
                    .unwrap(),
            }
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            window.navigate("potato://localhost".parse().unwrap())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

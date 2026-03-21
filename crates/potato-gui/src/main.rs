#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use potato_transport::{PotatoConnection, SseEvent};
use tauri::Manager;
use tauri::ipc::Channel;
use tauri::webview::WebviewWindowBuilder;

struct AppSocket(PotatoConnection);

#[tauri::command]
async fn create_call(
    state: tauri::State<'_, AppSocket>,
    body: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    state
        .0
        .stream("POST", "/calls", Some(body.as_bytes()), |event| {
            let json = match &event {
                SseEvent::Started { call_id } => {
                    serde_json::json!({"event": "started", "data": {"call_id": call_id}})
                }
                SseEvent::Output(data) => serde_json::json!({"event": "output", "data": data}),
                SseEvent::Error(data) => serde_json::json!({"event": "error", "data": data}),
                SseEvent::End => serde_json::json!({"event": "end"}),
            };
            let _ = on_event.send(json.to_string());
        })
        .await;

    Ok(())
}

#[tauri::command]
async fn send_call_stdin(
    state: tauri::State<'_, AppSocket>,
    call_id: String,
    data: String,
) -> Result<String, String> {
    let path = format!("/calls/{call_id}/stdin");
    let response = state
        .0
        .fetch("POST", &path, Some(data.as_bytes()))
        .await
        .map_err(|e| e.to_string())?;
    String::from_utf8(response).map_err(|e| format!("invalid response: {e}"))
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

fn activate_app(app_name: &str) {
    let server = PotatoConnection::new("/tmp/potato.sock");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let body = serde_json::json!({ "image": app_name });
    let response = rt
        .block_on(server.fetch("POST", "/activate", Some(body.to_string().as_bytes())))
        .unwrap_or_else(|e| {
            eprintln!("failed to activate app (is potato-server running?): {e}");
            std::process::exit(1);
        });

    let result: serde_json::Value = serde_json::from_slice(&response).unwrap_or_default();
    if result.get("ok") != Some(&serde_json::Value::Bool(true)) {
        eprintln!("failed to activate app: {result}");
        std::process::exit(1);
    }
}

fn main() {
    let app_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: potato-app <app-name>");
        std::process::exit(1);
    });

    activate_app(&app_name);

    let app_socket = PotatoConnection::new(format!("/tmp/potato-{app_name}.sock"));
    let protocol_socket = app_socket.clone();

    tauri::Builder::default()
        .register_uri_scheme_protocol("potato", move |_ctx, request| {
            let mut path = request.uri().path().to_string();

            if path == "/" || path.is_empty() {
                path = "/index.html".to_string();
            }
            let server_path = format!("/files{path}");

            let rt = tokio::runtime::Runtime::new().unwrap();
            match rt.block_on(protocol_socket.fetch("GET", &server_path, None)) {
                Ok(response_body) => tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime_for_path(&path))
                    .body(response_body)
                    .unwrap(),
                Err(e) => tauri::http::Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(e.to_string().into_bytes())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![create_call, send_call_stdin])
        .setup(move |app| {
            app.manage(AppSocket(app_socket));

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

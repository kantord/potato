#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use spudkit_client::{SpudkitApp, SpudkitClient};
use tauri::Manager;
use tauri::ipc::Channel;
use tauri::webview::WebviewWindowBuilder;

struct AppState(SpudkitApp);

/// Generic forward: send a request to the app server and return the response.
#[tauri::command]
async fn forward(
    state: tauri::State<'_, AppState>,
    method: String,
    path: String,
    body: Option<String>,
    content_type: Option<String>,
) -> Result<String, String> {
    let body_bytes = body.as_deref().map(|b| b.as_bytes());
    let headers: Vec<(&str, &str)> = content_type
        .as_deref()
        .map(|ct| vec![("Content-Type", ct)])
        .unwrap_or_default();
    let response = state
        .0
        .forward(&method, &path, body_bytes, &headers)
        .await
        .map_err(|e| e.to_string())?;
    String::from_utf8(response).map_err(|e| format!("invalid response: {e}"))
}

/// Send stdin data to a running call.
#[tauri::command]
async fn send_stdin(
    state: tauri::State<'_, AppState>,
    call_id: String,
    data: serde_json::Value,
) -> Result<(), String> {
    state
        .0
        .send_stdin(&call_id, &data)
        .await
        .map_err(|e| e.to_string())
}

/// Generic stream: send a request and stream events back via Channel.
#[tauri::command]
async fn stream(
    state: tauri::State<'_, AppState>,
    method: String,
    path: String,
    body: Option<String>,
    on_event: Channel<String>,
) -> Result<(), String> {
    let body_bytes = body.as_deref().map(|b| b.as_bytes());
    state
        .0
        .stream_forward(&method, &path, body_bytes, |event| {
            let _ = on_event.send(event.to_json());
        })
        .await
        .map_err(|e| e.to_string())
}

fn main() {
    let app_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: spud-app-tauri <app-name>");
        std::process::exit(1);
    });

    let client = SpudkitClient::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(client.app(&app_name)).unwrap_or_else(|e| {
        eprintln!("failed to activate app: {e}");
        std::process::exit(1);
    });
    let protocol_app = app.clone();
    let protocol_rt = std::sync::Arc::new(rt);

    tauri::Builder::default()
        .register_uri_scheme_protocol("spudkit", move |_ctx, request| {
            let mut path = request.uri().path().to_string();

            if path == "/" || path.is_empty() {
                path = "/index.html".to_string();
            }

            match protocol_rt.block_on(protocol_app.fetch_file(&path)) {
                Ok(response_body) => tauri::http::Response::builder()
                    .status(200)
                    .header(
                        "Content-Type",
                        mime_guess::from_path(&path)
                            .first_or_octet_stream()
                            .as_ref(),
                    )
                    .body(response_body)
                    .expect("valid HTTP response"),
                Err(e) => tauri::http::Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(e.to_string().into_bytes())
                    .expect("valid HTTP response"),
            }
        })
        .invoke_handler(tauri::generate_handler![forward, stream, send_stdin])
        .setup(move |tauri_app| {
            tauri_app.manage(AppState(app));

            let polyfill = include_str!("../dist/polyfill.js");

            let window = WebviewWindowBuilder::new(tauri_app, "main", Default::default())
                .title("SpudKit")
                .inner_size(800.0, 600.0)
                .initialization_script(polyfill)
                .build()?;

            window.navigate("spudkit://localhost".parse().unwrap())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

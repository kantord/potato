#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use potato_client::{PotatoApp, PotatoClient};
use tauri::Manager;
use tauri::ipc::Channel;
use tauri::webview::WebviewWindowBuilder;

struct AppState(PotatoApp);

#[tauri::command]
async fn create_call(
    state: tauri::State<'_, AppState>,
    body: String,
    on_event: Channel<String>,
) -> Result<(), String> {
    let cmd: Vec<String> = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("cmd")?
                .as_array()?
                .iter()
                .map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    state
        .0
        .call(&cmd, |event| {
            let _ = on_event.send(event.to_json());
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn send_call_stdin(
    state: tauri::State<'_, AppState>,
    call_id: String,
    data: String,
) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("invalid JSON: {e}"))?;
    let stdin_data = value
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    state
        .0
        .send_stdin(&call_id, &stdin_data)
        .await
        .map_err(|e| e.to_string())
}

fn main() {
    let app_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: potato-app <app-name>");
        std::process::exit(1);
    });

    let client = PotatoClient::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(client.app(&app_name)).unwrap_or_else(|e| {
        eprintln!("failed to activate app: {e}");
        std::process::exit(1);
    });
    let protocol_app = app.clone();
    let protocol_rt = std::sync::Arc::new(rt);

    tauri::Builder::default()
        .register_uri_scheme_protocol("potato", move |_ctx, request| {
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
        .invoke_handler(tauri::generate_handler![create_call, send_call_stdin])
        .setup(move |tauri_app| {
            tauri_app.manage(AppState(app));

            let polyfill = include_str!("../frontend/potato-polyfill.js");

            let window = WebviewWindowBuilder::new(tauri_app, "main", Default::default())
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

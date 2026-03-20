#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

fn fetch_from_socket(path: &str) -> Result<Vec<u8>, String> {
    let mut stream = UnixStream::connect("/tmp/potato.sock")
        .map_err(|e| format!("failed to connect to socket: {e}"))?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())
        .map_err(|e| format!("failed to write: {e}"))?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)
        .map_err(|e| format!("failed to read: {e}"))?;

    if let Some(pos) = String::from_utf8_lossy(&response).find("\r\n\r\n") {
        Ok(response[pos + 4..].to_vec())
    } else {
        Ok(response)
    }
}

#[tauri::command]
fn load_file(path: String) -> Result<String, String> {
    let bytes = fetch_from_socket(&path)?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![load_file])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

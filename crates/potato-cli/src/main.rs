use anyhow::{Context, bail};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;

fn http_request(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> anyhow::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect to {socket_path}"))?;

    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(b) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes())?;
    if let Some(b) = body {
        stream.write_all(b)?;
    }

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;

    if let Some(pos) = String::from_utf8_lossy(&response).find("\r\n\r\n") {
        Ok(response[pos + 4..].to_vec())
    } else {
        Ok(response)
    }
}

/// Streams SSE from a request. Reports call_id via started_tx when "started" event arrives.
fn stream_sse(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    started_tx: Option<std::sync::mpsc::Sender<String>>,
) {
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to connect: {e}");
            std::process::exit(1);
        }
    };

    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(b) = body {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).unwrap();
    if let Some(b) = body {
        stream.write_all(b).unwrap();
    }

    let reader = BufReader::new(stream);
    let mut past_headers = false;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if !past_headers {
            if line.is_empty() {
                past_headers = true;
            }
            continue;
        }

        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data.is_empty() {
                continue;
            }

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                let event = parsed
                    .get("event")
                    .and_then(|e| e.as_str())
                    .unwrap_or("output");

                match event {
                    "started" => {
                        if let Some(ref tx) = started_tx {
                            let call_id =
                                parsed["data"]["call_id"].as_str().unwrap_or("").to_string();
                            let _ = tx.send(call_id);
                        }
                    }
                    "end" => break,
                    "error" => {
                        if let Some(d) = parsed.get("data") {
                            eprintln!("{}", format_data(d));
                        }
                    }
                    _ => {
                        if let Some(d) = parsed.get("data") {
                            println!("{}", format_data(d));
                        }
                    }
                }
            }
        }
    }
}

fn format_data(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn activate(app_name: &str) -> anyhow::Result<()> {
    let body = serde_json::json!({ "image": app_name });
    let response = http_request(
        "/tmp/potato.sock",
        "POST",
        "/activate",
        Some(body.to_string().as_bytes()),
    )
    .context("is potato-server running?")?;

    let result: serde_json::Value = serde_json::from_slice(&response)?;
    if result.get("ok") != Some(&serde_json::Value::Bool(true)) {
        bail!("failed to activate app: {result}");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        bail!(
            "Usage: potato <app-name> <command> [args...]\nExample: potato potato-hello-simple /echo.sh"
        );
    }

    let app_name = &args[1];
    let cmd: Vec<String> = args[2..].to_vec();

    activate(app_name)?;

    let socket_path = format!("/tmp/potato-{app_name}.sock");

    let body = serde_json::json!({ "cmd": cmd });
    let socket_for_stream = socket_path.clone();
    let socket_for_stdin = socket_path.clone();
    let body_bytes = body.to_string().into_bytes();

    let (started_tx, started_rx) = std::sync::mpsc::channel::<String>();

    let output_handle = thread::spawn(move || {
        stream_sse(
            &socket_for_stream,
            "POST",
            "/calls",
            Some(&body_bytes),
            Some(started_tx),
        );
    });

    let call_id = match started_rx.recv() {
        Ok(id) => id,
        Err(_) => {
            let _ = output_handle.join();
            return Ok(());
        }
    };

    let stdin_path = format!("/calls/{call_id}/stdin");
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let body = serde_json::json!({ "data": { "text": line } });
        let _ = http_request(
            &socket_for_stdin,
            "POST",
            &stdin_path,
            Some(body.to_string().as_bytes()),
        );
    }

    let _ = output_handle.join();
    Ok(())
}

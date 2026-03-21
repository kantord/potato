use anyhow::{Context, bail};
use potato_transport::{SseEvent, http_request, stream_sse};
use std::io::BufRead;

fn format_data(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

async fn activate(app_name: &str) -> anyhow::Result<()> {
    let body = serde_json::json!({ "image": app_name });
    let response = http_request(
        "/tmp/potato.sock",
        "POST",
        "/activate",
        Some(body.to_string().as_bytes()),
    )
    .await
    .context("is potato-server running?")?;

    let result: serde_json::Value = serde_json::from_slice(&response)?;
    if result.get("ok") != Some(&serde_json::Value::Bool(true)) {
        bail!("failed to activate app: {result}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        bail!(
            "Usage: potato <app-name> <command> [args...]\nExample: potato potato-hello-simple /echo.sh"
        );
    }

    let app_name = &args[1];
    let cmd: Vec<String> = args[2..].to_vec();

    activate(app_name).await?;

    let socket_path = format!("/tmp/potato-{app_name}.sock");

    let body = serde_json::json!({ "cmd": cmd });
    let socket_for_stream = socket_path.clone();
    let socket_for_stdin = socket_path.clone();
    let body_bytes = body.to_string().into_bytes();

    let (started_tx, started_rx) = std::sync::mpsc::channel::<String>();

    let output_handle = tokio::spawn(async move {
        stream_sse(
            &socket_for_stream,
            "POST",
            "/calls",
            Some(&body_bytes),
            |event| match event {
                SseEvent::Started { call_id } => {
                    let _ = started_tx.send(call_id);
                }
                SseEvent::Output(data) => println!("{}", format_data(&data)),
                SseEvent::Error(data) => eprintln!("{}", format_data(&data)),
                SseEvent::End => {}
            },
        )
        .await;
    });

    let call_id = match started_rx.recv() {
        Ok(id) => id,
        Err(_) => {
            let _ = output_handle.await;
            return Ok(());
        }
    };

    let stdin_path = format!("/calls/{call_id}/stdin");

    // Read stdin in a blocking thread
    let stdin_handle = tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines
    });

    let lines = stdin_handle.await?;
    for line in lines {
        let body = serde_json::json!({ "data": { "text": line } });
        let _ = http_request(
            &socket_for_stdin,
            "POST",
            &stdin_path,
            Some(body.to_string().as_bytes()),
        )
        .await;
    }

    let _ = output_handle.await;
    Ok(())
}

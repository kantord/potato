use anyhow::bail;
use potato_client::{PotatoClient, SseEvent};
use std::io::BufRead;

fn format_data(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
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

    let client = PotatoClient::new();
    let app = client.app(app_name).await?;
    let app_for_stdin = app.clone();

    let (started_tx, started_rx) = std::sync::mpsc::channel::<String>();

    let output_handle = tokio::spawn(async move {
        app.call(&cmd, |event| match event {
            SseEvent::Started { call_id } => {
                let _ = started_tx.send(call_id);
            }
            SseEvent::Output(data) => println!("{}", format_data(&data)),
            SseEvent::Error(data) => eprintln!("{}", format_data(&data)),
            SseEvent::End => {}
        })
        .await;
    });

    let call_id = match started_rx.recv() {
        Ok(id) => id,
        Err(_) => {
            let _ = output_handle.await;
            return Ok(());
        }
    };

    let stdin_handle = tokio::task::spawn_blocking(move || {
        let stdin = std::io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines
    });

    let lines = stdin_handle.await?;
    for line in lines {
        let data = serde_json::json!({ "text": line });
        let _ = app_for_stdin.send_stdin(&call_id, &data).await;
    }

    let _ = output_handle.await;
    Ok(())
}

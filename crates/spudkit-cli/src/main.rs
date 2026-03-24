use clap::Parser;
use spudkit_client::{SpudkitClient, SseEvent};
use std::io::BufRead;

/// Run containerized apps from the command line
#[derive(Parser)]
#[command(name = "spud", version, about)]
struct Args {
    /// Docker image name of the app
    app: String,

    /// Command to run inside the container
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let client = SpudkitClient::new();
    let app = client.app(&args.app).await?;
    let app_for_stdin = app.clone();

    let (call_id_tx, call_id_rx) = tokio::sync::oneshot::channel::<String>();

    let cmd = args.command;
    let output_handle = tokio::spawn(async move {
        let mut call_id_tx = Some(call_id_tx);
        app.call(&cmd, |event| match &event {
            SseEvent::Started { call_id } => {
                if let Some(tx) = call_id_tx.take() {
                    let _ = tx.send(call_id.clone());
                }
            }
            SseEvent::Error(_) => {
                if let Some(text) = event.display_data() {
                    eprintln!("{text}");
                }
            }
            SseEvent::End => {}
            _ => {
                if let Some(text) = event.display_data() {
                    println!("{text}");
                }
            }
        })
        .await
        .unwrap_or_else(|e| {
            eprintln!("failed to connect: {e}");
            std::process::exit(1);
        });
    });

    let call_id = match call_id_rx.await {
        Ok(id) => id,
        Err(_) => {
            let _ = output_handle.await;
            return Ok(());
        }
    };

    // Forward stdin line by line concurrently with output
    let stdin_handle = tokio::spawn(async move {
        let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(32);

        // Read stdin in a blocking thread
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(l) => {
                        if line_tx.blocking_send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Send each line as it arrives
        while let Some(line) = line_rx.recv().await {
            let data = serde_json::json!({ "text": line });
            let _ = app_for_stdin.send_stdin(&call_id, &data).await;
        }
    });

    // Wait for output to finish (process exit closes the stream)
    let _ = output_handle.await;
    stdin_handle.abort();
    Ok(())
}

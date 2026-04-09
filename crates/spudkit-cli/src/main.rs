mod build;

use clap::{Parser, Subcommand};
use spudkit_client::{SpudkitClient, SseEvent};
use std::io::BufRead;

#[derive(Parser)]
#[command(name = "spud", version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a spud image and tag it as `spud-<name>`
    Build {
        /// Image tag or spud name
        #[arg(short = 't', long = "tag", value_name = "TAG")]
        tag: String,
        /// Build context path
        #[arg(value_name = "PATH")]
        path: std::path::PathBuf,
    },
    /// Run a command inside a spud
    Run {
        /// Name of the spud
        app: String,
        /// Command to run inside the container
        command: Vec<String>,
    },
    /// Open a spud in a GUI window
    App {
        /// Name of the spud
        name: String,
    },
    /// List available spuds
    Ls,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Build { tag, path } => build::run(&tag, &path).await,
        Command::Run { app, command } => run(&app, command).await,
        Command::App { name } => app(&name),
        Command::Ls => ls().await,
    }
}

fn find_frontend() -> Option<&'static str> {
    let candidates = ["spud-app-chromium", "spud-app-tauri"];
    candidates
        .into_iter()
        .find(|&candidate| which::which(candidate).is_ok())
}

fn app(name: &str) -> anyhow::Result<()> {
    let frontend = find_frontend()
        .ok_or_else(|| anyhow::anyhow!("no GUI frontend found. Install spud-app-tauri."))?;

    let status = std::process::Command::new(frontend).arg(name).status()?;

    if !status.success() {
        anyhow::bail!("{frontend} exited with {status}");
    }
    Ok(())
}

async fn ls() -> anyhow::Result<()> {
    let client = SpudkitClient::new();
    let spuds = client.list_spuds().await?;
    for spud in spuds {
        println!("{}", spud.name());
    }
    Ok(())
}

async fn run(app: &str, cmd: Vec<String>) -> anyhow::Result<()> {
    let client = SpudkitClient::new();
    let app = client.app(app).await?;
    let app_for_stdin = app.clone();

    let (call_id_tx, call_id_rx) = tokio::sync::oneshot::channel::<String>();

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

    let stdin_handle = tokio::spawn(async move {
        let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(32);

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

        while let Some(line) = line_rx.recv().await {
            let data = serde_json::json!({ "text": line });
            let _ = app_for_stdin.send_stdin(&call_id, &data).await;
        }
    });

    let _ = output_handle.await;
    stdin_handle.abort();
    Ok(())
}

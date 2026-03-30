use spudkit_client::SpudkitClient;
use spudkit_core::Spud;
use std::process::Command;
use tokio::net::{TcpListener, UnixStream};

fn find_chrome() -> anyhow::Result<String> {
    let candidates = [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
    ];
    for candidate in candidates {
        if which::which(candidate).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    anyhow::bail!("no Chrome/Chromium binary found. Install chromium or google-chrome.")
}

async fn run_proxy(listener: TcpListener, socket_path: String) {
    loop {
        let Ok((mut tcp_stream, _)) = listener.accept().await else {
            break;
        };
        let path = socket_path.clone();
        tokio::spawn(async move {
            let Ok(mut unix_stream) = UnixStream::connect(&path).await else {
                return;
            };
            let (mut tcp_read, mut tcp_write) = tcp_stream.split();
            let (mut unix_read, mut unix_write) = unix_stream.split();
            let _ = tokio::select! {
                r = tokio::io::copy(&mut tcp_read, &mut unix_write) => r,
                r = tokio::io::copy(&mut unix_read, &mut tcp_write) => r,
            };
        });
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: spud-app-chromium <app-name>");
        std::process::exit(1);
    });

    let chrome = find_chrome()?;

    // Activate the app via the spudkit server
    let client = SpudkitClient::new();
    let _app = client.app(&app_name).await?;

    // Get the unix socket path
    let spud = Spud::new(&app_name)?;
    let socket_path = spud.socket_path();

    // Start TCP proxy on a random loopback port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let proxy_handle = tokio::spawn(run_proxy(listener, socket_path));

    // Create temp profile directory
    let profile_dir = tempfile::tempdir()?;

    // Launch Chrome in app mode
    let mut child = Command::new(&chrome)
        .arg(format!("--user-data-dir={}", profile_dir.path().display()))
        .arg(format!("--app=http://127.0.0.1:{port}"))
        .spawn()?;

    // Wait for Chrome to exit
    let status = child.wait()?;

    // Shut down the proxy
    proxy_handle.abort();

    if !status.success() {
        anyhow::bail!("chrome exited with {status}");
    }

    Ok(())
}

use clap::Parser;

/// SpudKit server — manages containerized apps via per-app Unix sockets
#[derive(Parser)]
#[command(name = "spudkit-server", version, about)]
struct Args {}

#[tokio::main]
async fn main() {
    Args::parse();

    let manager = spudkit_server::start("/tmp/spudkit.sock").await;

    println!("Ready. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await.unwrap();

    println!("Shutting down...");
    manager.shutdown().await;
}

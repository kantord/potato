use clap::Parser;

/// Potato server — manages containerized apps via per-app Unix sockets
#[derive(Parser)]
#[command(name = "potato-server", version, about)]
struct Args {}

#[tokio::main]
async fn main() {
    Args::parse();

    let manager = potato_server::start("/tmp/potato.sock").await;

    println!("Ready. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await.unwrap();

    println!("Shutting down...");
    manager.shutdown().await;
}

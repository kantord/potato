use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let registry: potato_server::AppRegistry = Arc::new(Mutex::new(HashMap::new()));

    let mgmt_path = "/tmp/potato.sock";
    let _ = std::fs::remove_file(mgmt_path);
    let mgmt_listener = UnixListener::bind(mgmt_path).unwrap();
    println!("Potato server listening on {mgmt_path}");

    let mgmt_app = potato_server::management_app(registry.clone());

    tokio::spawn(async move {
        axum::serve(mgmt_listener, mgmt_app).await.unwrap();
    });

    println!("Ready. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await.unwrap();

    println!("Shutting down...");
    let apps = registry.lock().await;
    for (name, app) in apps.iter() {
        if let Some(id) = &app.container_id {
            println!("[{name}] Stopping container...");
            potato_server::stop_container(id).await;
        }
        let path = format!("/tmp/potato-{name}.sock");
        let _ = std::fs::remove_file(&path);
    }
}

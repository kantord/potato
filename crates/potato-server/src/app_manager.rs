use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::container;

pub struct RunningApp {
    pub container_id: Option<String>,
}

/// Manages the set of active apps and their containers.
#[derive(Clone, Default)]
pub struct AppManager {
    apps: Arc<Mutex<HashMap<String, RunningApp>>>,
}

impl AppManager {
    pub fn new() -> Self {
        Self {
            apps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn contains(&self, name: &str) -> bool {
        self.apps.lock().await.contains_key(name)
    }

    pub async fn insert(&self, name: String, app: RunningApp) {
        self.apps.lock().await.insert(name, app);
    }

    pub async fn list(&self) -> Vec<String> {
        self.apps.lock().await.keys().cloned().collect()
    }

    /// Stop all running containers and remove app sockets.
    pub async fn shutdown(&self) {
        let apps = self.apps.lock().await;
        for (name, app) in apps.iter() {
            if let Some(id) = &app.container_id {
                println!("[{name}] Stopping container...");
                container::stop_container(id).await;
            }
            let path = format!("/tmp/potato-{name}.sock");
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Start the management socket and return the manager.
pub async fn start(mgmt_path: &str) -> AppManager {
    let manager = AppManager::new();

    let _ = std::fs::remove_file(mgmt_path);
    let listener = tokio::net::UnixListener::bind(mgmt_path).unwrap();
    println!("Potato server listening on {mgmt_path}");

    let mgmt_app = crate::api::management_app(manager.clone());
    tokio::spawn(async move {
        axum::serve(listener, mgmt_app).await.unwrap();
    });

    manager
}

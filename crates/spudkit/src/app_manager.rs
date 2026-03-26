use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::container::{AppContainer, SpudkitImage};

pub struct RunningApp {
    pub container: AppContainer,
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

    /// Activate an app: extract its image, start a container, and serve it on a Unix socket.
    /// Returns "already_active" if the app is already running.
    pub async fn activate(&self, image: &str) -> anyhow::Result<&'static str> {
        if self.apps.lock().await.contains_key(image) {
            return Ok("already_active");
        }

        let spudkit_image = SpudkitImage::new(image).await?;

        let static_dir = spudkit_image.extract().await?;

        let container = spudkit_image.start().await?;
        let container_id = container.id.clone();

        let path = format!("/tmp/spudkit-{image}.sock");
        let _ = std::fs::remove_file(&path);

        let listener = tokio::net::UnixListener::bind(&path)?;

        let router = crate::app_router(static_dir, container_id);
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        self.apps
            .lock()
            .await
            .insert(image.to_string(), RunningApp { container });

        Ok("activated")
    }

    pub async fn list(&self) -> Vec<String> {
        self.apps.lock().await.keys().cloned().collect()
    }

    /// Stop all running containers and remove app sockets.
    pub async fn shutdown(&self) {
        let apps = self.apps.lock().await;
        for (name, app) in apps.iter() {
            println!("[{name}] Stopping container...");
            app.container.stop().await;
            let path = format!("/tmp/spudkit-{name}.sock");
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Start the management socket and return the manager.
pub async fn start(mgmt_path: &str) -> AppManager {
    let manager = AppManager::new();

    let _ = std::fs::remove_file(mgmt_path);
    let listener = tokio::net::UnixListener::bind(mgmt_path).unwrap();
    println!("SpudKit server listening on {mgmt_path}");

    let mgmt_app = crate::api::spudkit_router(manager.clone());
    tokio::spawn(async move {
        axum::serve(listener, mgmt_app).await.unwrap();
    });

    manager
}

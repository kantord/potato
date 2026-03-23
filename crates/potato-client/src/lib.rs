use anyhow::{Context, bail};

use potato_transport::PotatoConnection;
pub use potato_transport::SseEvent;

const MANAGEMENT_SOCKET: &str = "/tmp/potato.sock";

/// Client for interacting with the potato server.
/// Handles app activation and provides connections to individual apps.
#[derive(Clone)]
pub struct PotatoClient {
    server: PotatoConnection,
}

impl Default for PotatoClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PotatoClient {
    pub fn new() -> Self {
        Self {
            server: PotatoConnection::new(MANAGEMENT_SOCKET),
        }
    }

    async fn activate(&self, app_name: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({ "image": app_name });
        let response = self
            .server
            .fetch("POST", "/activate", Some(body.to_string().as_bytes()))
            .await
            .context("is potato-server running?")?;

        let result: serde_json::Value = serde_json::from_slice(&response)?;
        if result.get("ok") != Some(&serde_json::Value::Bool(true)) {
            bail!("failed to activate app: {result}");
        }
        Ok(())
    }

    /// Activate an app and return a connection to it.
    /// Idempotent — safe to call multiple times.
    pub async fn app(&self, app_name: &str) -> anyhow::Result<PotatoApp> {
        self.activate(app_name).await?;
        Ok(PotatoApp {
            conn: PotatoConnection::new(format!("/tmp/potato-{app_name}.sock")),
        })
    }
}

/// A connection to a specific potato app.
#[derive(Clone)]
pub struct PotatoApp {
    conn: PotatoConnection,
}

impl PotatoApp {
    /// Start a call and stream events. The `on_event` callback receives each event
    /// including the initial `Started` event with the `call_id`.
    pub async fn call(&self, cmd: &[String], on_event: impl FnMut(SseEvent)) -> anyhow::Result<()> {
        let body = serde_json::json!({ "cmd": cmd });
        self.conn
            .stream(
                "POST",
                "/calls",
                Some(body.to_string().as_bytes()),
                on_event,
            )
            .await
    }

    /// Send input to a running call.
    pub async fn send_stdin(&self, call_id: &str, data: &serde_json::Value) -> anyhow::Result<()> {
        let path = format!("/calls/{call_id}/stdin");
        let body = serde_json::json!({ "data": data });
        self.conn
            .fetch("POST", &path, Some(body.to_string().as_bytes()))
            .await?;
        Ok(())
    }

    /// Forward a raw request to the app server.
    pub async fn forward(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        headers: &[(&str, &str)],
    ) -> anyhow::Result<Vec<u8>> {
        self.conn
            .fetch_with_headers(method, path, body, headers)
            .await
    }

    /// Forward a raw request and stream events via callback.
    pub async fn stream_forward(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        on_event: impl FnMut(SseEvent),
    ) -> anyhow::Result<()> {
        self.conn.stream(method, path, body, on_event).await
    }

    /// Fetch a static file from the app.
    pub async fn fetch_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let server_path = format!("/files{path}");
        self.conn.fetch("GET", &server_path, None).await
    }
}

use anyhow::{Context, bail};

pub use spudkit_core::Spud;
use spudkit_core::SpudkitConnection;
pub use spudkit_core::SseEvent;

const MANAGEMENT_SOCKET: &str = "/tmp/spudkit.sock";

/// Client for interacting with the spudkit server.
/// Handles app activation and provides connections to individual apps.
#[derive(Clone)]
pub struct SpudkitClient {
    server: SpudkitConnection,
}

impl Default for SpudkitClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SpudkitClient {
    pub fn new() -> Self {
        Self {
            server: SpudkitConnection::new(MANAGEMENT_SOCKET),
        }
    }

    async fn activate(&self, spud: &Spud) -> anyhow::Result<()> {
        let body = serde_json::json!({ "name": spud.name() });
        let response = self
            .server
            .fetch("POST", "/activate", Some(body.to_string().as_bytes()))
            .await
            .context("is spudkit running?")?;

        let result: serde_json::Value = serde_json::from_slice(&response)?;
        if result.get("ok") != Some(&serde_json::Value::Bool(true)) {
            bail!("failed to activate app: {result}");
        }
        Ok(())
    }

    /// Activate an app and return a connection to it.
    /// Idempotent — safe to call multiple times.
    pub async fn app(&self, app_name: &str) -> anyhow::Result<SpudkitApp> {
        let spud = Spud::new(app_name)?;
        self.activate(&spud).await?;
        Ok(SpudkitApp {
            conn: SpudkitConnection::new(spud.socket_path()),
        })
    }

    /// List all locally available spuds.
    pub async fn list_spuds(&self) -> anyhow::Result<Vec<Spud>> {
        let response = self
            .server
            .fetch("GET", "/spuds", None)
            .await
            .context("is spudkit running?")?;

        let result: serde_json::Value = serde_json::from_slice(&response)?;
        let names = result["spuds"].as_array().context("expected spuds array")?;

        let mut spuds = Vec::new();
        for name in names {
            if let Some(s) = name.as_str()
                && let Ok(spud) = Spud::new(s)
            {
                spuds.push(spud);
            }
        }
        Ok(spuds)
    }
}

/// A connection to a specific spudkit app.
#[derive(Clone)]
pub struct SpudkitApp {
    conn: SpudkitConnection,
}

impl SpudkitApp {
    /// Start a call and stream events. The `on_event` callback receives each event
    /// including the initial `Started` event with the `call_id`.
    pub async fn call(&self, cmd: &[String], on_event: impl FnMut(SseEvent)) -> anyhow::Result<()> {
        let body = serde_json::json!({ "cmd": cmd });
        self.conn
            .stream(
                "POST",
                "/__calls",
                Some(body.to_string().as_bytes()),
                on_event,
            )
            .await
    }

    /// Send input to a running call.
    pub async fn send_stdin(&self, call_id: &str, data: &serde_json::Value) -> anyhow::Result<()> {
        let path = format!("/__calls/{call_id}/stdin");
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
        let server_path = format!("/__files{path}");
        self.conn.fetch("GET", &server_path, None).await
    }
}

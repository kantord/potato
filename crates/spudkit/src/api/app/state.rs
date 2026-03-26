use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

pub(crate) type StdinWriter = Arc<Mutex<Option<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>>>;

#[derive(Clone)]
pub(crate) struct AppState {
    pub container_id: String,
    pub static_dir: std::path::PathBuf,
    pub stdin_writers: Arc<Mutex<HashMap<String, StdinWriter>>>,
}

impl AppState {
    /// Write a line to a call's stdin. Retries briefly if the process is still starting.
    pub async fn write_stdin(&self, call_id: &str, data: &[u8]) -> bool {
        for _ in 0..20 {
            {
                let writers = self.stdin_writers.lock().await;
                if let Some(writer) = writers.get(call_id) {
                    let mut guard = writer.lock().await;
                    if let Some(ref mut w) = *guard
                        && w.write_all(data).await.is_ok()
                    {
                        return true;
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        false
    }
}

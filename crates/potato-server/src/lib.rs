use axum::{Json, Router, extract::State, routing::post};
use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::StreamExt;
use std::path::PathBuf;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    container_id: String,
}

#[derive(serde::Deserialize)]
struct RunRequest {
    cmd: Vec<String>,
}

async fn run_command(State(state): State<AppState>, Json(body): Json<RunRequest>) -> String {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => return format!("error connecting to docker: {e}\n"),
    };

    let exec = match docker
        .create_exec(
            &state.container_id,
            CreateExecOptions {
                cmd: Some(body.cmd),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            },
        )
        .await
    {
        Ok(e) => e,
        Err(e) => return format!("error creating exec: {e}\n"),
    };

    match docker.start_exec(&exec.id, None).await {
        Ok(StartExecResults::Attached { mut output, .. }) => {
            let mut result = String::new();
            while let Some(Ok(log)) = output.next().await {
                result.push_str(&log.to_string());
            }
            result
        }
        Ok(StartExecResults::Detached) => String::new(),
        Err(e) => format!("error running exec: {e}\n"),
    }
}

pub async fn start_container(image: &str) -> Result<String, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let name = format!("potato-{}", uuid());
    let container = docker
        .create_container(
            Some(CreateContainerOptions { name: Some(name), ..Default::default() }),
            config,
        )
        .await?;

    docker.start_container(&container.id, None).await?;

    Ok(container.id)
}

pub async fn stop_container(container_id: &str) {
    if let Ok(docker) = Docker::connect_with_local_defaults() {
        let _ = docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions { force: true, ..Default::default() }),
            )
            .await;
    }
}

pub async fn extract_image(image: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["true".to_string()]),
        ..Default::default()
    };

    let name = format!("potato-extract-{}", uuid());
    let container = docker
        .create_container(
            Some(CreateContainerOptions { name: Some(name), ..Default::default() }),
            config,
        )
        .await?;

    let mut tar_stream = docker.export_container(&container.id);
    let mut tar_bytes = Vec::new();
    while let Some(chunk) = tar_stream.next().await {
        tar_bytes.extend_from_slice(&chunk?);
    }

    let _ = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions { force: true, ..Default::default() }),
        )
        .await;

    let extract_dir = std::env::temp_dir().join(format!("potato-{}", uuid()));
    std::fs::create_dir_all(&extract_dir)?;

    let mut archive = tar::Archive::new(&tar_bytes[..]);
    archive.unpack(&extract_dir)?;

    Ok(extract_dir)
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{n:x}")
}

pub fn app(static_dir: PathBuf, container_id: String) -> Router {
    let state = AppState { container_id };
    Router::new()
        .route("/run", post(run_command))
        .nest_service("/files", ServeDir::new(static_dir))
        .with_state(state)
}

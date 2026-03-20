use axum::{Router, extract::Path, routing::get};
use bollard::Docker;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, LogsOptions, RemoveContainerOptions};
use futures_util::StreamExt;
use std::path::PathBuf;
use tower_http::services::ServeDir;

async fn run_command(Path(command): Path<String>) -> String {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => return format!("error connecting to docker: {e}\n"),
    };

    let config = ContainerCreateBody {
        image: Some("debian:bookworm-slim".to_string()),
        cmd: Some(vec![command]),
        ..Default::default()
    };

    let name = format!("potato-{}", uuid());
    let container = match docker
        .create_container(
            Some(CreateContainerOptions { name: Some(name), ..Default::default() }),
            config,
        )
        .await
    {
        Ok(c) => c,
        Err(e) => return format!("error creating container: {e}\n"),
    };

    if let Err(e) = docker.start_container(&container.id, None).await {
        let _ = docker.remove_container(&container.id, None).await;
        return format!("error starting container: {e}\n");
    }

    let wait = docker.wait_container(&container.id, None);
    let _: Vec<_> = wait.collect().await;

    let mut output = String::new();
    let mut logs = docker.logs(
        &container.id,
        Some(LogsOptions {
            follow: false,
            stdout: true,
            stderr: true,
            ..Default::default()
        }),
    );

    while let Some(Ok(log)) = logs.next().await {
        output.push_str(&log.to_string());
    }

    let _ = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions { force: true, ..Default::default() }),
        )
        .await;

    output
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

pub fn app(static_dir: PathBuf) -> Router {
    Router::new()
        .route("/run/{command}", get(run_command))
        .nest_service("/files", ServeDir::new(static_dir))
}

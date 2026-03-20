use axum::{Router, extract::Path, routing::get};
use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, LogsOptions, RemoveContainerOptions};
use futures_util::StreamExt;

async fn run_command(Path(command): Path<String>) -> String {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => return format!("error connecting to docker: {e}\n"),
    };

    let config = Config {
        image: Some("debian:bookworm-slim"),
        cmd: Some(vec![&command]),
        ..Default::default()
    };

    let name = format!("potato-{}", uuid());
    let container = match docker
        .create_container(Some(CreateContainerOptions { name: name.as_str(), platform: None }), config)
        .await
    {
        Ok(c) => c,
        Err(e) => return format!("error creating container: {e}\n"),
    };

    if let Err(e) = docker.start_container::<String>(&container.id, None).await {
        let _ = docker.remove_container(&container.id, None).await;
        return format!("error starting container: {e}\n");
    }

    let wait = docker.wait_container::<String>(&container.id, None);
    let _: Vec<_> = wait.collect().await;

    let mut output = String::new();
    let mut logs = docker.logs::<String>(
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

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{n:x}")
}

pub fn app() -> Router {
    Router::new().route("/run/{command}", get(run_command))
}

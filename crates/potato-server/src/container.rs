use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::{Stream, StreamExt};
use std::path::PathBuf;
use std::pin::Pin;

/// A running app container.
pub struct AppContainer {
    pub id: String,
}

/// The attached streams from an exec call.
pub struct ExecAttached {
    pub output: Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>,
    pub input: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
}

impl AppContainer {
    /// Start a persistent container for an app image.
    pub async fn start(image: &str) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;

        let config = ContainerCreateBody {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let name = format!("potato-{}", crate::utils::generate_id());
        let container = docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(name),
                    ..Default::default()
                }),
                config,
            )
            .await?;

        docker.start_container(&container.id, None).await?;

        Ok(Self { id: container.id })
    }

    /// Execute a command in the container and return attached stdin/stdout/stderr.
    pub async fn exec(&self, cmd: Vec<String>) -> anyhow::Result<ExecAttached> {
        let docker = Docker::connect_with_local_defaults()?;

        let exec = docker
            .create_exec(
                &self.id,
                CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        match docker.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { output, input } => Ok(ExecAttached {
                output: Box::pin(output),
                input: Box::new(input),
            }),
            StartExecResults::Detached => {
                anyhow::bail!("exec started in detached mode")
            }
        }
    }

    /// Stop and remove the container.
    pub async fn stop(&self) {
        if let Ok(docker) = Docker::connect_with_local_defaults() {
            let _ = docker
                .remove_container(
                    &self.id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
    }
}

/// Extract an image's filesystem to a temp directory.
pub async fn extract_image(image: &str) -> anyhow::Result<PathBuf> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["true".to_string()]),
        ..Default::default()
    };

    let name = format!("potato-extract-{}", crate::utils::generate_id());
    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: Some(name),
                ..Default::default()
            }),
            config,
        )
        .await?;

    let extract_dir = std::env::temp_dir().join(format!("potato-{}", crate::utils::generate_id()));
    std::fs::create_dir_all(&extract_dir)?;

    let (pipe_reader, mut pipe_writer) = os_pipe::pipe()?;
    let extract_dir_clone = extract_dir.clone();

    let unpack_handle = std::thread::spawn(
        move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let mut archive = tar::Archive::new(pipe_reader);
            archive.set_preserve_permissions(false);
            archive.set_unpack_xattrs(false);
            for entry in archive.entries()? {
                let mut entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let kind = entry.header().entry_type();
                if kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link() {
                    let _ = entry.unpack_in(&extract_dir_clone);
                }
            }
            Ok(())
        },
    );

    let mut tar_stream = docker.export_container(&container.id);
    while let Some(chunk) = tar_stream.next().await {
        let chunk = chunk?;
        if std::io::Write::write_all(&mut pipe_writer, &chunk).is_err() {
            break;
        }
    }
    drop(pipe_writer);

    unpack_handle
        .join()
        .map_err(|_| anyhow::anyhow!("unpack thread panicked"))?
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let _ = docker
        .remove_container(
            &container.id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;

    Ok(extract_dir)
}

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::{Stream, StreamExt};
use std::path::PathBuf;
use std::pin::Pin;

const SPUDKIT_LABEL: &str = "io.github.kantord.spudkit.version";

/// A validated spudkit container image.
pub struct SpudkitImage {
    name: String,
}

impl SpudkitImage {
    /// Validate that an image carries the spudkit label.
    pub async fn new(image: &str) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        let info = docker.inspect_image(image).await?;

        let has_label = info
            .config
            .as_ref()
            .and_then(|c| c.labels.as_ref())
            .and_then(|labels| labels.get(SPUDKIT_LABEL))
            .is_some();

        if !has_label {
            anyhow::bail!(
                "image {image} is not a spudkit container: missing label {SPUDKIT_LABEL}"
            );
        }

        Ok(Self {
            name: image.to_string(),
        })
    }

    /// Start a persistent container for this image.
    pub async fn start(&self) -> anyhow::Result<AppContainer> {
        let docker = Docker::connect_with_local_defaults()?;

        let config = ContainerCreateBody {
            image: Some(self.name.clone()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let name = format!("spudkit-{}", crate::utils::generate_id());
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

        Ok(AppContainer { id: container.id })
    }

    /// Extract the image's filesystem to a temp directory.
    pub async fn extract(&self) -> anyhow::Result<PathBuf> {
        extract_image_inner(&self.name).await
    }
}

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
    /// Start a container without label validation. For tests only.
    #[doc(hidden)]
    pub async fn start_unchecked(image: &str) -> anyhow::Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;

        let config = ContainerCreateBody {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };

        let name = format!("spudkit-{}", crate::utils::generate_id());
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

    /// Run a command, send optional stdin, and collect all stdout lines.
    pub async fn run(
        &self,
        cmd: Vec<String>,
        stdin_data: Option<&serde_json::Value>,
    ) -> anyhow::Result<Vec<String>> {
        use tokio::io::AsyncWriteExt;

        let attached = self.exec(cmd).await?;

        let mut input = attached.input;
        if let Some(data) = stdin_data {
            let line = serde_json::to_string(data).unwrap() + "\n";
            let _ = input.write_all(line.as_bytes()).await;
        }
        let _ = input.shutdown().await;
        drop(input);

        let mut lines = Vec::new();
        let mut output = attached.output;
        while let Some(Ok(log)) = output.next().await {
            let text = match &log {
                LogOutput::StdOut { message } => String::from_utf8_lossy(message).to_string(),
                _ => continue,
            };
            for line in text.lines() {
                if !line.is_empty() {
                    lines.push(line.to_string());
                }
            }
        }

        Ok(lines)
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

async fn extract_image_inner(image: &str) -> anyhow::Result<PathBuf> {
    let docker = Docker::connect_with_local_defaults()?;

    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        cmd: Some(vec!["true".to_string()]),
        ..Default::default()
    };

    let name = format!("spudkit-extract-{}", crate::utils::generate_id());
    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: Some(name),
                ..Default::default()
            }),
            config,
        )
        .await?;

    let extract_dir = std::env::temp_dir().join(format!("spudkit-{}", crate::utils::generate_id()));
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

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};
use futures_util::{Stream, StreamExt};
use spudkit_core::Spud;
use std::pin::Pin;

const SPUDKIT_LABEL: &str = "io.github.kantord.spudkit.version";

/// A validated spudkit container image.
pub struct SpudkitImage {
    spud: Spud,
}

impl SpudkitImage {
    /// The Docker image name derived from the spud (e.g., "spud-hello-world").
    pub fn image_name(&self) -> String {
        format!("spud-{}", self.spud.name())
    }

    /// Validate that a spud's image carries the spudkit label.
    pub async fn from_spud(spud: Spud) -> anyhow::Result<Self> {
        let image_name = format!("spud-{}", spud.name());
        let docker = Docker::connect_with_local_defaults()?;
        let info = docker.inspect_image(&image_name).await?;

        let has_label = info
            .config
            .as_ref()
            .and_then(|c| c.labels.as_ref())
            .and_then(|labels| labels.get(SPUDKIT_LABEL))
            .is_some();

        if !has_label {
            anyhow::bail!(
                "image {image_name} is not a spudkit container: missing label {SPUDKIT_LABEL}"
            );
        }

        Ok(Self { spud })
    }

    pub fn spud(&self) -> &Spud {
        &self.spud
    }

    /// List all locally available spuds (images with the spudkit label and `spud-` prefix).
    pub async fn list_available() -> anyhow::Result<Vec<Spud>> {
        use std::collections::HashMap;

        let docker = Docker::connect_with_local_defaults()?;
        let mut filters = HashMap::new();
        filters.insert("label", vec![SPUDKIT_LABEL]);
        let options = bollard::query_parameters::ListImagesOptionsBuilder::default()
            .filters(&filters)
            .build();

        let images = docker.list_images(Some(options)).await?;

        let mut spuds = Vec::new();
        for image in images {
            for tag in &image.repo_tags {
                let name = tag.split(':').next().unwrap_or(tag);
                if let Some(short_name) = name.strip_prefix("spud-")
                    && let Ok(spud) = Spud::new(short_name)
                {
                    spuds.push(spud);
                }
            }
        }

        Ok(spuds)
    }

    /// Start a persistent container for this image.
    pub async fn start(&self) -> anyhow::Result<AppContainer> {
        let docker = Docker::connect_with_local_defaults()?;

        let config = ContainerCreateBody {
            image: Some(self.image_name()),
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
}

/// A running app container.
#[derive(Clone)]
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

    /// Read a file from the container as raw bytes.
    /// Returns `Ok(None)` if the file does not exist.
    pub async fn cat_file(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let docker = Docker::connect_with_local_defaults()?;

        let exec = docker
            .create_exec(
                &self.id,
                CreateExecOptions::<String> {
                    cmd: Some(vec!["cat".into(), path.into()]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let exec_id = exec.id.clone();

        match docker.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut bytes = Vec::new();
                while let Some(Ok(log)) = output.next().await {
                    if let LogOutput::StdOut { message } = log {
                        bytes.extend_from_slice(&message);
                    }
                }
                let inspect = docker.inspect_exec(&exec_id).await?;
                if inspect.exit_code == Some(0) {
                    Ok(Some(bytes))
                } else {
                    Ok(None)
                }
            }
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

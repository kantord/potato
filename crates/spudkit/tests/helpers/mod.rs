use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use tower::ServiceExt;

pub async fn install_file(
    container: &spudkit::container::AppContainer,
    path: &str,
    content: &[u8],
) {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let dir = std::path::Path::new(path)
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    let cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("mkdir -p {dir} && cat > {path}"),
    ];
    let attached = container.exec(cmd).await.unwrap();
    let mut input = attached.input;
    input.write_all(content).await.unwrap();
    input.shutdown().await.unwrap();
    drop(input);
    let mut output = attached.output;
    while output.next().await.is_some() {}
}

pub async fn app_with_script(script_name: &str, script_content: &str) -> axum::Router {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    let install_cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!(
            "mkdir -p /app/bin && cat > /app/bin/{script_name} && chmod +x /app/bin/{script_name}"
        ),
    ];
    let attached = container.exec(install_cmd).await.unwrap();
    use tokio::io::AsyncWriteExt;
    let mut input = attached.input;
    input.write_all(script_content.as_bytes()).await.unwrap();
    input.shutdown().await.unwrap();
    drop(input);
    let mut output = attached.output;
    use futures_util::StreamExt;
    while output.next().await.is_some() {}

    spudkit::app_router(container)
}

pub async fn app_with_script_and_template(
    script_name: &str,
    script_content: &str,
    template_content: &[u8],
) -> axum::Router {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    let install_cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!(
            "mkdir -p /app/bin && cat > /app/bin/{script_name} && chmod +x /app/bin/{script_name}"
        ),
    ];
    let attached = container.exec(install_cmd).await.unwrap();
    use tokio::io::AsyncWriteExt;
    let mut input = attached.input;
    input.write_all(script_content.as_bytes()).await.unwrap();
    input.shutdown().await.unwrap();
    drop(input);
    let mut output = attached.output;
    use futures_util::StreamExt;
    while output.next().await.is_some() {}

    let template_name = format!("{script_name}.html");
    install_file(
        &container,
        &format!("/app/templates/{template_name}"),
        template_content,
    )
    .await;

    spudkit::app_router(container)
}

pub fn parse_sse_events(body: &str) -> Vec<serde_json::Value> {
    body.lines()
        .filter(|line| line.starts_with("data:"))
        .filter_map(|line| {
            let json = line.strip_prefix("data:")?.trim();
            serde_json::from_str(json).ok()
        })
        .collect()
}

pub async fn call_and_get_events(app: axum::Router, cmd: Vec<&str>) -> Vec<serde_json::Value> {
    let cmd_json: Vec<String> = cmd.into_iter().map(|s| s.to_string()).collect();
    let body = serde_json::json!({ "cmd": cmd_json });

    let response = app
        .oneshot(
            Request::post("/calls")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        response.into_body().collect(),
    )
    .await
    .expect("timed out waiting for events")
    .unwrap();

    let text = String::from_utf8(body.to_bytes().to_vec()).unwrap();
    parse_sse_events(&text)
}

pub fn non_started_events(events: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    events
        .into_iter()
        .filter(|e| e.get("event").and_then(|v| v.as_str()) != Some("started"))
        .collect()
}

pub fn build_labeled_image(name: &str) {
    let dockerfile =
        "FROM debian:bookworm-slim\nLABEL io.github.kantord.spudkit.version=\"1\"".to_string();
    let output = std::process::Command::new("docker")
        .args(["build", "-t", name, "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(dockerfile.as_bytes())?;
            child.wait()
        });
    assert!(output.is_ok(), "failed to build test image {name}");
}

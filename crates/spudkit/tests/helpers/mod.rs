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
            Request::post("/_api/calls")
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

pub async fn call_with_stdin_and_get_events(
    app: axum::Router,
    cmd: Vec<&str>,
    stdin_data: serde_json::Value,
) -> Vec<serde_json::Value> {
    let app_for_stdin = app.clone();
    let cmd_json: Vec<String> = cmd.into_iter().map(|s| s.to_string()).collect();

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<serde_json::Value>(32);

    tokio::spawn(async move {
        let response = app
            .oneshot(
                Request::post("/_api/calls")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "cmd": cmd_json }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let mut body = response.into_body();
        let mut buffer = String::new();

        loop {
            match body.frame().await {
                None => break,
                Some(Err(_)) => break,
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        buffer.push_str(&String::from_utf8_lossy(&data));
                    }
                }
            }

            while let Some(pos) = buffer.find("\n\n") {
                let chunk = buffer[..pos + 2].to_string();
                buffer = buffer[pos + 2..].to_string();
                for event in parse_sse_events(&chunk) {
                    if event_tx.send(event).await.is_err() {
                        return;
                    }
                }
            }
        }

        for event in parse_sse_events(&buffer) {
            let _ = event_tx.send(event).await;
        }
    });

    let mut all_events = vec![];
    let mut stdin_sent = false;

    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), event_rx.recv())
            .await
            .expect("timeout waiting for SSE event");

        let Some(event) = event else { break };

        if !stdin_sent {
            if let Some(call_id) = event
                .get("data")
                .and_then(|d| d.get("call_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                app_for_stdin
                    .clone()
                    .oneshot(
                        Request::post(format!("/_api/calls/{call_id}/stdin"))
                            .header("Content-Type", "application/json")
                            .body(Body::from(
                                serde_json::json!({ "data": stdin_data }).to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                stdin_sent = true;
            }
        }

        let is_end = event.get("event").and_then(|v| v.as_str()) == Some("end");
        all_events.push(event);
        if is_end {
            break;
        }
    }

    all_events
}

pub fn build_labeled_image(name: &str) {
    build_labeled_image_with_extra(name, "");
}

pub fn build_labeled_image_with_extra(name: &str, extra_labels: &str) {
    let mut dockerfile = concat!(
        "FROM debian:bookworm-slim\n",
        "LABEL io.github.kantord.spudkit.version=\"1\"\n",
        "RUN apt-get update && apt-get install -y --no-install-recommends s6",
        " && rm -rf /var/lib/apt/lists/*\n",
        "RUN mkdir -p /app/gui /app/bin /app/templates /run/spudkit\n",
        "RUN printf '#!/bin/sh\\nread -r cmd\\ncase \"$cmd\" in\\n",
        "  */*|*..*)  exit 1 ;;\\nesac\\nexec \"/app/bin/$cmd\"\\n'",
        " > /usr/local/bin/dispatch && chmod +x /usr/local/bin/dispatch\n",
        "CMD [\"s6-ipcserver\", \"-P\", \"-c\", \"200\",",
        " \"/run/spudkit/exec.sock\", \"/usr/local/bin/dispatch\"]",
    )
    .to_string();
    if !extra_labels.is_empty() {
        dockerfile.push_str(&format!("\n{extra_labels}"));
    }
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

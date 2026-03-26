use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use rstest::*;
use std::path::PathBuf;
use tower::ServiceExt;

#[fixture]
async fn app() -> axum::Router {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");
    spudkit::app_router(dir, container.id)
}

fn parse_sse_events(body: &str) -> Vec<serde_json::Value> {
    body.lines()
        .filter(|line| line.starts_with("data:"))
        .filter_map(|line| {
            let json = line.strip_prefix("data:")?.trim();
            serde_json::from_str(json).ok()
        })
        .collect()
}

async fn call_and_get_events(app: axum::Router, cmd: Vec<&str>) -> Vec<serde_json::Value> {
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

fn non_started_events(events: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    events
        .into_iter()
        .filter(|e| e.get("event").and_then(|v| v.as_str()) != Some("started"))
        .collect()
}

// --- /calls tests ---

// Commands with absolute paths bypass resolve_cmd
#[rstest]
#[case::date(vec!["/bin/date"], "output", None)]
#[case::echo(vec!["/bin/echo", "hello"], "output", Some(serde_json::json!("hello")))]
#[case::stderr(vec!["/bin/sh", "-c", "echo oops >&2"], "error", None)]
#[case::pretagged(
    vec!["/bin/echo", r#"{"event":"progress","data":{"percent":50}}"#],
    "progress",
    Some(serde_json::json!({"percent": 50}))
)]
#[tokio::test]
async fn call_produces_expected_event(
    #[future] app: axum::Router,
    #[case] cmd: Vec<&str>,
    #[case] expected_event: &str,
    #[case] expected_data: Option<serde_json::Value>,
) {
    let events = non_started_events(call_and_get_events(app.await, cmd).await);
    assert!(!events.is_empty(), "expected events");
    assert_eq!(events[0]["event"], expected_event);
    if let Some(data) = expected_data {
        assert_eq!(events[0]["data"], data);
    }
}

#[rstest]
#[tokio::test]
async fn call_ends_with_end_event(#[future] app: axum::Router) {
    let events = non_started_events(call_and_get_events(app.await, vec!["/bin/echo", "hi"]).await);
    let last = events.last().unwrap();
    assert_eq!(last["event"], "end");
}

#[rstest]
#[tokio::test]
async fn call_started_event_contains_call_id(#[future] app: axum::Router) {
    let events = call_and_get_events(app.await, vec!["/bin/echo", "hi"]).await;
    let started = events
        .iter()
        .find(|e| e["event"] == "started")
        .expect("no started event");
    assert!(started["data"]["call_id"].is_string());
}

// --- /render tests ---

/// Helper to create a test app with a script installed in /app/bin/
async fn app_with_script(script_name: &str, script_content: &str) -> axum::Router {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    // Install the script into the container
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
    // Wait for the command to finish
    let mut output = attached.output;
    use futures_util::StreamExt;
    while output.next().await.is_some() {}

    spudkit::app_router(dir, container.id)
}

#[tokio::test]
async fn render_returns_plain_text_without_template() {
    // date.sh has no matching template in fixtures
    let app = app_with_script("date.sh", "#!/bin/sh\ndate").await;

    let response = app
        .oneshot(
            Request::post("/render/date.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.is_empty());
}

#[tokio::test]
async fn render_with_template_returns_html() {
    // echo.sh has a matching template at fixtures/app/templates/echo.html
    let app = app_with_script("echo.sh", "#!/bin/sh\necho hello\necho world").await;

    let response = app
        .oneshot(
            Request::post("/render/echo.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("<p>hello</p>"));
    assert!(text.contains("<p>world</p>"));
}

#[tokio::test]
async fn render_accepts_form_encoded_data() {
    let app = app_with_script("cat.sh", "#!/bin/sh\ncat").await;

    let response = app
        .oneshot(
            Request::post("/render/cat.sh")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from("name=alice&color=blue"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("alice"));
    assert!(text.contains("blue"));
}

#[tokio::test]
async fn render_accepts_json_with_data_field() {
    let app = app_with_script("cat.sh", "#!/bin/sh\ncat").await;

    let response = app
        .oneshot(
            Request::post("/render/cat.sh")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"data": {"greeting": "hi"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("greeting"));
    assert!(text.contains("hi"));
}

#[tokio::test]
async fn render_nonexistent_script_returns_error() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");
    let app = spudkit::app_router(dir, container.id);

    let response = app
        .oneshot(
            Request::post("/render/nonexistent.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    // The exec fails — either returns error status or error message in body
    assert!(
        status >= 400
            || text.contains("no such file")
            || text.contains("not found")
            || text.contains("exec failed"),
        "expected error for nonexistent script, got status={status} body={text}"
    );
}

// --- label validation tests ---

#[tokio::test]
async fn spudkit_image_rejects_unlabeled_image() {
    let result = spudkit::container::SpudkitImage::new("debian:bookworm-slim").await;
    match result {
        Ok(_) => panic!("expected SpudkitImage::new to reject unlabeled image"),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spudkit") || msg.contains("label"),
                "error should mention missing label, got: {msg}"
            );
        }
    }
}

// --- /files tests ---

#[rstest]
#[tokio::test]
async fn unknown_route_returns_404(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[rstest]
#[tokio::test]
async fn files_serves_static_content(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(
            Request::get("/files/index.html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("<h1>test</h1>"));
}

#[rstest]
#[tokio::test]
async fn files_returns_404_for_missing_file(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(
            Request::get("/files/nonexistent.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

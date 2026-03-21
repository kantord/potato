use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use std::path::PathBuf;
use tower::ServiceExt;

async fn test_app() -> axum::Router {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let container_id = potato_server::start_container("debian:bookworm-slim")
        .await
        .expect("failed to start container");
    potato_server::app(dir, Some(container_id))
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
    let events = parse_sse_events(&text);

    // Filter out the "started" event for cleaner assertions
    events
        .into_iter()
        .filter(|e| e.get("event").and_then(|v| v.as_str()) != Some("started"))
        .collect()
}

#[tokio::test]
async fn call_date_returns_output() {
    let app = test_app().await;
    let events = call_and_get_events(app, vec!["date"]).await;
    assert!(!events.is_empty(), "expected events");
    assert_eq!(events[0]["event"], "output");
}

#[tokio::test]
async fn call_echo_returns_output() {
    let app = test_app().await;
    let events = call_and_get_events(app, vec!["echo", "hello"]).await;
    assert!(!events.is_empty(), "expected events");
    assert_eq!(events[0]["event"], "output");
    assert_eq!(events[0]["data"], "hello");
}

#[tokio::test]
async fn call_stderr_tagged_as_error() {
    let app = test_app().await;
    let events = call_and_get_events(app, vec!["sh", "-c", "echo oops >&2"]).await;
    assert!(!events.is_empty(), "expected events");
    assert_eq!(events[0]["event"], "error");
}

#[tokio::test]
async fn call_pretagged_event_passed_through() {
    let app = test_app().await;
    let events = call_and_get_events(
        app,
        vec!["echo", r#"{"event":"progress","data":{"percent":50}}"#],
    )
    .await;
    assert!(!events.is_empty(), "expected events");
    assert_eq!(events[0]["event"], "progress");
    assert_eq!(events[0]["data"]["percent"], 50);
}

#[tokio::test]
async fn call_ends_with_end_event() {
    let app = test_app().await;
    let events = call_and_get_events(app, vec!["echo", "hi"]).await;
    let last = events.last().unwrap();
    assert_eq!(last["event"], "end");
}

#[tokio::test]
async fn call_started_event_contains_call_id() {
    let app = test_app().await;
    let cmd_json = vec!["echo".to_string(), "hi".to_string()];
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
    .expect("timed out")
    .unwrap();

    let text = String::from_utf8(body.to_bytes().to_vec()).unwrap();
    let all_events = parse_sse_events(&text);
    let started = all_events.iter().find(|e| e["event"] == "started").expect("no started event");
    assert!(started["data"]["call_id"].is_string());
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = test_app().await;

    let response = app
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn files_serves_static_content() {
    let app = test_app().await;

    let response = app
        .oneshot(Request::get("/files/Cargo.toml").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("[package]"));
}

#[tokio::test]
async fn files_returns_404_for_missing_file() {
    let app = test_app().await;

    let response = app
        .oneshot(Request::get("/files/nonexistent.txt").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

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
    potato_server::app(dir, container_id)
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

#[tokio::test]
async fn run_date_returns_sse_output() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::post("/run")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"cmd":["date"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&text);
    assert!(!events.is_empty(), "expected SSE events, got: {text}");
    assert_eq!(events[0]["event"], "output");
}

#[tokio::test]
async fn run_with_stdin() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::post("/run")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"cmd":["cat"],"stdin":{"hello":"world"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&text);
    assert!(!events.is_empty(), "expected SSE events, got: {text}");
    assert_eq!(events[0]["event"], "output");
    assert_eq!(events[0]["data"]["hello"], "world");
}

#[tokio::test]
async fn run_stderr_tagged_as_error() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::post("/run")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"cmd":["sh","-c","echo oops >&2"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&text);
    assert!(!events.is_empty(), "expected SSE events, got: {text}");
    assert_eq!(events[0]["event"], "error");
}

#[tokio::test]
async fn run_pretagged_event_passed_through() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::post("/run")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"cmd":["echo","{\"event\":\"progress\",\"data\":{\"percent\":50}}"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_sse_events(&text);
    assert!(!events.is_empty(), "expected SSE events, got: {text}");
    assert_eq!(events[0]["event"], "progress");
    assert_eq!(events[0]["data"]["percent"], 50);
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

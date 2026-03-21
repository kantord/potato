use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use rstest::*;
use std::path::PathBuf;
use tower::ServiceExt;

#[fixture]
async fn app() -> axum::Router {
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
    parse_sse_events(&text)
}

fn non_started_events(events: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    events
        .into_iter()
        .filter(|e| e.get("event").and_then(|v| v.as_str()) != Some("started"))
        .collect()
}

#[rstest]
#[case::date(vec!["date"], "output", None)]
#[case::echo(vec!["echo", "hello"], "output", Some(serde_json::json!("hello")))]
#[case::stderr(vec!["sh", "-c", "echo oops >&2"], "error", None)]
#[case::pretagged(
    vec!["echo", r#"{"event":"progress","data":{"percent":50}}"#],
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
    let events = non_started_events(call_and_get_events(app.await, vec!["echo", "hi"]).await);
    let last = events.last().unwrap();
    assert_eq!(last["event"], "end");
}

#[rstest]
#[tokio::test]
async fn call_started_event_contains_call_id(#[future] app: axum::Router) {
    let events = call_and_get_events(app.await, vec!["echo", "hi"]).await;
    let started = events
        .iter()
        .find(|e| e["event"] == "started")
        .expect("no started event");
    assert!(started["data"]["call_id"].is_string());
}

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
            Request::get("/files/Cargo.toml")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("[package]"));
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

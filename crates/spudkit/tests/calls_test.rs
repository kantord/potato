#[allow(dead_code)]
mod helpers;

use helpers::{call_and_get_events, non_started_events};
use rstest::*;

#[fixture]
async fn app() -> axum::Router {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    spudkit::app_router(container)
}

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

#[allow(dead_code)]
mod helpers;

use helpers::{app_with_script, call_and_get_events, call_with_stdin_and_get_events};
use rstest::*;

const READBACK_SCRIPT: &str = "#!/bin/sh\nread line\necho \"$line\"\n";

#[fixture]
async fn app() -> axum::Router {
    app_with_script("readback.sh", READBACK_SCRIPT).await
}

#[rstest]
#[tokio::test]
async fn concurrent_calls_produce_independent_output(#[future] app: axum::Router) {
    let app = app.await;
    let n = 5;

    let handles: Vec<_> = (0..n)
        .map(|i| {
            let app_clone = app.clone();
            tokio::spawn(async move {
                let events = call_with_stdin_and_get_events(
                    app_clone,
                    vec!["/app/bin/readback.sh"],
                    serde_json::json!({"id": i}),
                )
                .await;
                (i, events)
            })
        })
        .collect();

    for handle in handles {
        let (i, events) = handle.await.unwrap();
        let output_events: Vec<_> = events.iter().filter(|e| e["event"] == "output").collect();

        assert!(
            !output_events.is_empty(),
            "call {i} produced no output events"
        );

        assert_eq!(
            output_events[0]["data"],
            serde_json::json!({"id": i}),
            "call {i} got wrong data back (calls may have mixed up stdin)"
        );
    }
}

#[rstest]
#[tokio::test]
async fn concurrent_calls_all_complete(#[future] app: axum::Router) {
    let app = app.await;
    let n = 5;

    let handles: Vec<_> = (0..n)
        .map(|i| {
            let app_clone = app.clone();
            tokio::spawn(async move {
                call_with_stdin_and_get_events(
                    app_clone,
                    vec!["/app/bin/readback.sh"],
                    serde_json::json!({"id": i}),
                )
                .await
            })
        })
        .collect();

    for handle in handles {
        let events = handle.await.unwrap();
        assert!(
            events.iter().any(|e| e["event"] == "end"),
            "a concurrent call did not produce an end event"
        );
    }
}

#[rstest]
#[tokio::test]
async fn concurrent_calls_via_exec_are_isolated(#[future] app: axum::Router) {
    let app = app.await;

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let app_clone = app.clone();
            tokio::spawn(async move {
                (
                    i,
                    call_and_get_events(app_clone, vec!["/bin/echo", &format!("call-{i}")]).await,
                )
            })
        })
        .collect();

    for handle in handles {
        let (i, events) = handle.await.unwrap();
        let output_events: Vec<_> = events.iter().filter(|e| e["event"] == "output").collect();

        assert!(!output_events.is_empty(), "call {i} produced no output");
        assert_eq!(
            output_events[0]["data"],
            format!("call-{i}"),
            "call {i} got wrong output"
        );
    }
}

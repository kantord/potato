#[allow(dead_code)]
mod helpers;

use helpers::{app_with_script, call_with_stdin_and_get_events, non_started_events};
use rstest::*;

const READBACK_SCRIPT: &str = "#!/bin/sh\nread line\necho \"$line\"\n";

#[fixture]
async fn app() -> axum::Router {
    app_with_script("readback.sh", READBACK_SCRIPT).await
}

#[rstest]
#[tokio::test]
async fn stdin_data_reaches_process(#[future] app: axum::Router) {
    let events = non_started_events(
        call_with_stdin_and_get_events(
            app.await,
            vec!["/app/bin/readback.sh"],
            serde_json::json!({"hello": "world"}),
        )
        .await,
    );

    let output_events: Vec<_> = events.iter().filter(|e| e["event"] == "output").collect();

    assert!(
        !output_events.is_empty(),
        "expected at least one output event"
    );
    assert_eq!(
        output_events[0]["data"],
        serde_json::json!({"hello": "world"})
    );
}

#[rstest]
#[tokio::test]
async fn stdin_is_delivered_before_process_exits(#[future] app: axum::Router) {
    let events = call_with_stdin_and_get_events(
        app.await,
        vec!["/app/bin/readback.sh"],
        serde_json::json!({"value": 42}),
    )
    .await;

    assert!(
        events.iter().any(|e| e["event"] == "end"),
        "stream should end after stdin is delivered"
    );
    assert!(
        events.iter().any(|e| e["event"] == "output"),
        "process should produce output after receiving stdin"
    );
}

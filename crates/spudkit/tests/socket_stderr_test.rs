//! Tests that stderr is propagated correctly through the unix socket path.
//!
//! These use `SpudkitImage::start()` so that `exec_socket_dir` is set and
//! `AppContainer::call()` routes through `call_via_socket()`.

#[allow(dead_code)]
mod helpers;

use helpers::{app_via_socket_with_script, call_and_get_events, non_started_events};
use rstest::*;

/// A script that writes to stderr and nothing to stdout.
const STDERR_ONLY_SCRIPT: &str = "#!/bin/sh\necho 'error output' >&2\n";

/// A script that writes to both stdout and stderr.
const MIXED_OUTPUT_SCRIPT: &str = "#!/bin/sh\necho 'standard output'\necho 'error output' >&2\n";

#[rstest]
#[tokio::test]
async fn socket_path_propagates_stderr() {
    let app =
        app_via_socket_with_script("spud-socket-stderr-test", "stderr-only", STDERR_ONLY_SCRIPT)
            .await;

    let events = non_started_events(call_and_get_events(app, vec!["stderr-only"]).await);

    let error_events: Vec<_> = events.iter().filter(|e| e["event"] == "error").collect();

    assert!(
        !error_events.is_empty(),
        "expected at least one 'error' SSE event from stderr, but got none. events: {events:?}"
    );
}

#[rstest]
#[tokio::test]
async fn socket_path_preserves_both_stdout_and_stderr() {
    let app = app_via_socket_with_script(
        "spud-socket-mixed-test",
        "mixed-output",
        MIXED_OUTPUT_SCRIPT,
    )
    .await;

    let events = non_started_events(call_and_get_events(app, vec!["mixed-output"]).await);

    let output_events: Vec<_> = events.iter().filter(|e| e["event"] == "output").collect();
    let error_events: Vec<_> = events.iter().filter(|e| e["event"] == "error").collect();

    assert!(
        !output_events.is_empty(),
        "expected at least one 'output' event from stdout"
    );

    assert!(
        !error_events.is_empty(),
        "expected at least one 'error' SSE event from stderr, but got none. events: {events:?}"
    );
}

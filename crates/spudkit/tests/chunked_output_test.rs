#[allow(dead_code)]
mod helpers;

use helpers::install_file;

/// When a script outputs many JSON lines, some will cross Docker's ~4KB chunk
/// boundaries. Each line collected by run() should still be a complete line,
/// not split at the chunk boundary.
#[tokio::test]
async fn run_does_not_split_lines_at_chunk_boundaries() {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    // Build a script that outputs 100 JSON lines at once via a single cat heredoc.
    // Each line is ~500 bytes. Writing 50KB in one burst makes it very likely
    // that Docker's stream framing splits mid-line.
    let padding = "x".repeat(400);
    let mut script = String::from("#!/bin/sh\ncat << 'HEREDOC'\n");
    for i in 1..=100 {
        script.push_str(&format!("{{\"id\":{i},\"data\":\"{padding}\"}}\n"));
    }
    script.push_str("HEREDOC\n");
    install_file(&container, "/app/bin/lines.sh", script.as_bytes()).await;

    // Make it executable
    let _ = container
        .run(
            vec!["/bin/chmod".into(), "+x".into(), "/app/bin/lines.sh".into()],
            None,
        )
        .await;

    let lines = container
        .run(vec!["/app/bin/lines.sh".into()], None)
        .await
        .unwrap();

    // Every line should be valid JSON with sequential ids
    let mut valid_count = 0;
    for (i, line) in lines.iter().enumerate() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(
            parsed.is_ok(),
            "line {} is not valid JSON (likely split at chunk boundary): {:?}",
            i + 1,
            line
        );
        valid_count += 1;
    }
    assert_eq!(
        valid_count, 100,
        "expected 100 valid JSON lines, got {valid_count}"
    );
}

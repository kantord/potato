/// The client should have a list_spuds() method that returns available spuds.
/// This test requires a running spudkit server.
#[tokio::test]
#[ignore] // requires a running spudkit server
async fn client_list_spuds_returns_spud_names() {
    // Ensure a spud- prefixed labeled image exists
    let _ = std::process::Command::new("docker")
        .args(["tag", "spudkit-base:latest", "spud-client-test:latest"])
        .output();

    let client = spudkit_client::SpudkitClient::new();
    let spuds = client.list_spuds().await.unwrap();

    let names: Vec<&str> = spuds.iter().map(|s| s.name()).collect();
    assert!(
        names.contains(&"client-test"),
        "expected client-test in {names:?}"
    );
}

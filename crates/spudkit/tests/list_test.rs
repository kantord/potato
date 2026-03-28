#[allow(dead_code)]
mod helpers;

use helpers::build_labeled_image;

#[tokio::test]
async fn list_available_spuds_returns_labeled_images() {
    build_labeled_image("spud-test-list");
    build_labeled_image("spud-test-list2");

    let spuds = spudkit::container::SpudkitImage::list_available()
        .await
        .unwrap();

    let names: Vec<&str> = spuds.iter().map(|s| s.name()).collect();
    assert!(
        names.contains(&"test-list"),
        "expected test-list in {names:?}"
    );
    assert!(
        names.contains(&"test-list2"),
        "expected test-list2 in {names:?}"
    );
}

#[tokio::test]
async fn list_available_spuds_excludes_non_spud_prefixed() {
    let spuds = spudkit::container::SpudkitImage::list_available()
        .await
        .unwrap();
    let names: Vec<&str> = spuds.iter().map(|s| s.name()).collect();
    assert!(
        !names.contains(&"spudkit-base"),
        "should not include non-spud-prefixed images: {names:?}"
    );
}

#[tokio::test]
async fn list_available_spuds_excludes_unlabeled() {
    let _ = std::process::Command::new("docker")
        .args(["tag", "debian:bookworm-slim", "spud-no-label:latest"])
        .output();

    let spuds = spudkit::container::SpudkitImage::list_available()
        .await
        .unwrap();
    let names: Vec<&str> = spuds.iter().map(|s| s.name()).collect();
    assert!(
        !names.contains(&"no-label"),
        "should not include unlabeled images: {names:?}"
    );
}

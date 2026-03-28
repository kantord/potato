use spudkit_core::Spud;

#[tokio::test]
async fn spudkit_image_rejects_unlabeled_image() {
    // debian:bookworm-slim doesn't have the spudkit label, but we need it
    // tagged as spud-unlabeled so SpudkitImage::from_spud can find it
    let _ = std::process::Command::new("docker")
        .args(["tag", "debian:bookworm-slim", "spud-unlabeled"])
        .output();

    let spud = Spud::new("unlabeled").unwrap();
    let result = spudkit::container::SpudkitImage::from_spud(spud).await;
    match result {
        Ok(_) => panic!("expected SpudkitImage::from_spud to reject unlabeled image"),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spudkit") || msg.contains("label"),
                "error should mention missing label, got: {msg}"
            );
        }
    }
}

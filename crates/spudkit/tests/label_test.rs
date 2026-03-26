#[tokio::test]
async fn spudkit_image_rejects_unlabeled_image() {
    let result = spudkit::container::SpudkitImage::new("debian:bookworm-slim").await;
    match result {
        Ok(_) => panic!("expected SpudkitImage::new to reject unlabeled image"),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("spudkit") || msg.contains("label"),
                "error should mention missing label, got: {msg}"
            );
        }
    }
}

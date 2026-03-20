use tokio::net::UnixListener;

#[tokio::main]
async fn main() {
    let image = "potato-hello-world";
    let path = "/tmp/potato.sock";
    let _ = std::fs::remove_file(path);

    println!("Extracting image filesystem...");
    let static_dir = potato_server::extract_image(image)
        .await
        .expect("failed to extract image");
    println!("Extracted to {}", static_dir.display());

    println!("Starting container...");
    let container_id = potato_server::start_container(image)
        .await
        .expect("failed to start container");
    println!("Container {container_id} running");

    let listener = UnixListener::bind(path).unwrap();
    println!("Listening on {path}");

    let result = axum::serve(listener, potato_server::app(static_dir, container_id.clone())).await;

    println!("Stopping container...");
    potato_server::stop_container(&container_id).await;

    result.unwrap();
}

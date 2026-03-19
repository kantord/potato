use axum::{Router, routing::get};
use tokio::net::UnixListener;

#[tokio::main]
async fn main() {
    let path = "/tmp/potato.sock";
    let _ = std::fs::remove_file(path);

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    let listener = UnixListener::bind(path).unwrap();
    println!("Listening on {path}");
    axum::serve(listener, app).await.unwrap();
}

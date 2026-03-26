#[allow(dead_code)]
mod helpers;

use axum::body::Body;
use helpers::install_file;
use http_body_util::BodyExt;
use hyper::Request;
use rstest::*;
use tower::ServiceExt;

#[fixture]
async fn app() -> axum::Router {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    install_file(&container, "/app/gui/index.html", b"<h1>test</h1>\n").await;

    spudkit::app_router(container.id)
}

#[rstest]
#[tokio::test]
async fn unknown_route_returns_404(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[rstest]
#[tokio::test]
async fn files_serves_static_content(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(
            Request::get("/files/index.html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("<h1>test</h1>"));
}

#[rstest]
#[tokio::test]
async fn files_returns_404_for_missing_file(#[future] app: axum::Router) {
    let response = app
        .await
        .oneshot(
            Request::get("/files/nonexistent.txt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn files_serves_binary_content() {
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC, 0x33, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    install_file(&container, "/app/gui/image.png", &png_bytes).await;

    let app = spudkit::app_router(container.id);

    let response = app
        .oneshot(
            Request::get("/files/image.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("image/png"),
        "expected image/png, got {content_type}"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.to_vec(), png_bytes);
}

#[tokio::test]
async fn files_traversal_cannot_read_etc_passwd() {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    install_file(&container, "/app/gui/index.html", b"hello").await;

    let app = spudkit::app_router(container.id);

    let paths = [
        "/files/..%2f..%2fetc%2fpasswd",
        "/files/..%252f..%252fetc%252fpasswd",
    ];

    for path in paths {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(
            !text.contains("root:"),
            "path {path} leaked /etc/passwd: {text}"
        );
    }
}

use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use std::path::PathBuf;
use tower::ServiceExt;

fn test_app() -> axum::Router {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    potato_server::app(dir)
}

#[tokio::test]
async fn run_date_returns_output() {
    let app = test_app();

    let response = app
        .oneshot(Request::get("/run/date").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.is_empty(), "expected date output, got empty string");
}

#[tokio::test]
async fn run_nonexistent_command_returns_error() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::get("/run/nonexistent_command_that_does_not_exist_abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("error") || text.contains("not found") || text.contains("No such file"),
        "expected error message, got: {text}"
    );
}

#[tokio::test]
async fn run_echo_returns_output() {
    let app = test_app();

    let response = app
        .oneshot(Request::get("/run/echo").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = test_app();

    let response = app
        .oneshot(Request::get("/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn files_serves_static_content() {
    let app = test_app();

    let response = app
        .oneshot(Request::get("/files/Cargo.toml").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("[package]"));
}

#[tokio::test]
async fn files_returns_404_for_missing_file() {
    let app = test_app();

    let response = app
        .oneshot(Request::get("/files/nonexistent.txt").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

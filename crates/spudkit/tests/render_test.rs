#[allow(dead_code)]
mod helpers;

use axum::body::Body;
use helpers::{app_with_script, app_with_script_and_template};
use http_body_util::BodyExt;
use hyper::Request;
use tower::ServiceExt;

#[tokio::test]
async fn render_returns_plain_text_without_template() {
    let app = app_with_script("date.sh", "#!/bin/sh\ndate").await;

    let response = app
        .oneshot(
            Request::post("/render/date.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!text.is_empty());
}

#[tokio::test]
async fn render_with_template_returns_html() {
    let app = app_with_script_and_template(
        "echo.sh",
        "#!/bin/sh\necho hello\necho world",
        b"{% for line in lines %}\n<p>{{ line }}</p>\n{% endfor %}\n",
    )
    .await;

    let response = app
        .oneshot(
            Request::post("/render/echo.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("<p>hello</p>"));
    assert!(text.contains("<p>world</p>"));
}

#[tokio::test]
async fn render_accepts_form_encoded_data() {
    let app = app_with_script("cat.sh", "#!/bin/sh\ncat").await;

    let response = app
        .oneshot(
            Request::post("/render/cat.sh")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from("name=alice&color=blue"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("alice"));
    assert!(text.contains("blue"));
}

#[tokio::test]
async fn render_accepts_json_with_data_field() {
    let app = app_with_script("cat.sh", "#!/bin/sh\ncat").await;

    let response = app
        .oneshot(
            Request::post("/render/cat.sh")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"data": {"greeting": "hi"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("greeting"));
    assert!(text.contains("hi"));
}

#[tokio::test]
async fn render_nonexistent_script_returns_error() {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");
    let app = spudkit::app_router(container);

    let response = app
        .oneshot(
            Request::post("/render/nonexistent.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        status >= 400
            || text.contains("no such file")
            || text.contains("not found")
            || text.contains("exec failed"),
        "expected error for nonexistent script, got status={status} body={text}"
    );
}

#[tokio::test]
async fn render_traversal_cannot_execute_arbitrary_binaries() {
    let container = spudkit::container::AppContainer::start_unchecked("debian:bookworm-slim")
        .await
        .expect("failed to start container");

    // Create /app/bin/ so the traversal path resolves
    helpers::install_file(&container, "/app/bin/.keep", b"").await;

    let app = spudkit::app_router(container);

    // ..%2f..%2f decodes to ../../ — so the script path becomes
    // /app/bin/../../bin/date → /bin/date
    // If traversal isn't blocked, this executes /bin/date successfully
    // and returns a date string instead of an error
    let response = app
        .oneshot(
            Request::post("/render/..%2f..%2fbin%2fdate")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    // If the traversal works, /bin/date runs and returns a date string (e.g. "Thu Mar 26")
    // If blocked, we get an error
    assert!(
        status >= 400 || text.contains("no such file") || text.contains("exec failed"),
        "path traversal in /render executed /bin/date: status={status} body={text}"
    );
}

#[tokio::test]
async fn render_auto_escapes_html_in_templates() {
    let app = app_with_script_and_template(
        "xss.sh",
        "#!/bin/sh\necho '{\"title\": \"<script>alert(1)</script>\"}'",
        b"<p>{{ title }}</p>",
    )
    .await;

    let response = app
        .oneshot(
            Request::post("/render/xss.sh")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        !text.contains("<script>"),
        "template should auto-escape HTML, but got raw script tag: {text}"
    );
    assert!(
        text.contains("&lt;script&gt;"),
        "expected escaped HTML entities, got: {text}"
    );
}

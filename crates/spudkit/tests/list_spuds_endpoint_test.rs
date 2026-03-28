#[allow(dead_code)]
mod helpers;

use axum::body::Body;
use helpers::build_labeled_image;
use http_body_util::BodyExt;
use hyper::Request;
use tower::ServiceExt;

#[tokio::test]
async fn list_spuds_endpoint_returns_available_spuds() {
    build_labeled_image("spud-endpoint-test");

    let manager = spudkit::app_manager::AppManager::new();
    let app = spudkit::api::spudkit_router(manager);

    let response = app
        .oneshot(Request::get("/spuds").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spuds = json["spuds"].as_array().expect("expected spuds array");
    let names: Vec<&str> = spuds.iter().filter_map(|s| s.as_str()).collect();
    assert!(
        names.contains(&"endpoint-test"),
        "expected endpoint-test in {names:?}"
    );
}

#[tokio::test]
async fn list_spuds_endpoint_excludes_non_spud_prefixed() {
    let manager = spudkit::app_manager::AppManager::new();
    let app = spudkit::api::spudkit_router(manager);

    let response = app
        .oneshot(Request::get("/spuds").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let spuds = json["spuds"].as_array().expect("expected spuds array");
    let names: Vec<&str> = spuds.iter().filter_map(|s| s.as_str()).collect();
    assert!(
        !names.contains(&"spudkit-base"),
        "should not include non-spud-prefixed images: {names:?}"
    );
}

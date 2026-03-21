use anyhow::Context;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

fn build_client() -> Client<UnixConnector, Full<Bytes>> {
    Client::unix()
}

/// Send an HTTP request over a Unix socket and return the response body.
pub async fn http_request(
    socket_path: &str,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> anyhow::Result<Vec<u8>> {
    let client = build_client();
    let uri: hyper::Uri = Uri::new(socket_path, path).into();

    let method: Method = method.parse().context("invalid HTTP method")?;
    let body_bytes = body.unwrap_or(&[]);

    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Host", "localhost")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::copy_from_slice(body_bytes)))
        .context("failed to build request")?;

    let response = client.request(request).await.context("request failed")?;

    let body = response
        .into_body()
        .collect()
        .await
        .context("failed to read response body")?
        .to_bytes();

    Ok(body.to_vec())
}

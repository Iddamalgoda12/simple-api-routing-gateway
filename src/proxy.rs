use std::{collections::BTreeMap, time::Duration};

use axum::{body::Body, extract::State, http::{HeaderMap, HeaderValue, Request, StatusCode, header}, response::Response};
use reqwest::{Client, Url};
use tracing::warn;

#[derive(Clone)]
pub struct AppState {
    routes: BTreeMap<String, Url>,
    client: Client,
}

impl AppState {
    pub fn new(routes: BTreeMap<String, Url>, timeout: Duration) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { routes, client })
    }

    fn upstream_for(&self, path: &str) -> Option<(&str, &Url)> {
        self.routes.iter()
            .filter(|(prefix, _)| path == prefix.as_str() || path.starts_with(&format!("{}/", prefix.trim_end_matches('/'))))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(prefix, url)| (prefix.as_str(), url))
    }
}

pub async fn proxy_request(State(state): State<AppState>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let Some((prefix, upstream)) = state.upstream_for(parts.uri.path()) else {
        return error_response(StatusCode::NOT_FOUND, "no route matches this path");
    };

    let remaining_path = parts.uri.path().strip_prefix(prefix).unwrap_or("");
    let path = if remaining_path.is_empty() { "/" } else { remaining_path };
    let mut upstream_url = upstream.clone();
    upstream_url.set_path(path);
    upstream_url.set_query(parts.uri.query());

    let is_bodyless = parts.method == http::Method::GET || parts.method == http::Method::HEAD;
    let mut upstream_request = state.client
        .request(parts.method, upstream_url)
        .headers(filtered_headers(&parts.headers));
    if !is_bodyless {
        upstream_request = upstream_request.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    }
    let response = match upstream_request.send().await {
        Ok(response) => response,
        Err(error) => {
            warn!(%error, "upstream request failed");
            let status = if error.is_timeout() { StatusCode::GATEWAY_TIMEOUT } else { StatusCode::BAD_GATEWAY };
            return error_response(status, "upstream request failed");
        }
    };

    let status = response.status();
    let headers = filtered_headers(response.headers());
    let mut proxied = Response::new(Body::from_stream(response.bytes_stream()));
    *proxied.status_mut() = status;
    *proxied.headers_mut() = headers;
    proxied
}

fn filtered_headers(headers: &HeaderMap) -> HeaderMap {
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        if matches!(name.as_str(), "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization" | "te" | "trailer" | "transfer-encoding" | "upgrade" | "host" | "content-length") {
            continue;
        }
        result.append(name.clone(), value.clone());
    }
    result
}

fn error_response(status: StatusCode, message: &'static str) -> Response {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}

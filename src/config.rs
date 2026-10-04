use std::{collections::BTreeMap, env, fs, net::SocketAddr, time::Duration};

use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
struct RouteFile {
    routes: BTreeMap<String, String>,
}

pub struct GatewayConfig {
    pub listen_addr: String,
    pub request_timeout_secs: u64,
    pub max_request_body_bytes: usize,
    pub routes: BTreeMap<String, Url>,
}

impl GatewayConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let path = env::var("GATEWAY_CONFIG").unwrap_or_else(|_| "config/routes.yaml".to_owned());
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("could not read route config {path}: {error}"))?;
        let route_file: RouteFile = serde_saphyr::from_str(&content)
            .map_err(|error| format!("invalid route config {path}: {error}"))?;

        if route_file.routes.is_empty() {
            return Err("route config must contain at least one route".into());
        }

        let mut routes = BTreeMap::new();
        for (path_prefix, upstream_text) in route_file.routes {
            validate_prefix(&path_prefix)?;
            let mut upstream = Url::parse(&upstream_text)
                .map_err(|error| format!("invalid upstream for {path_prefix}: {error}"))?;
            if !matches!(upstream.scheme(), "http" | "https") || upstream.host_str().is_none() {
                return Err(format!("upstream for {path_prefix} must be an http or https URL with a host").into());
            }
            if upstream.path() != "/" || upstream.query().is_some() || upstream.fragment().is_some() {
                return Err(format!("upstream for {path_prefix} must be an origin URL without path/query/fragment").into());
            }
            upstream.set_path("");
            routes.insert(path_prefix, upstream);
        }

        let listen_addr = env::var("GATEWAY_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
        listen_addr.parse::<SocketAddr>()?;
        let request_timeout_secs = env::var("GATEWAY_REQUEST_TIMEOUT_SECS")
            .unwrap_or_else(|_| "15".to_owned()).parse::<u64>()?;
        if Duration::from_secs(request_timeout_secs).is_zero() {
            return Err("GATEWAY_REQUEST_TIMEOUT_SECS must be greater than zero".into());
        }
        let max_request_body_bytes = env::var("GATEWAY_MAX_BODY_BYTES")
            .unwrap_or_else(|_| "10485760".to_owned()).parse::<usize>()?;
        if max_request_body_bytes == 0 {
            return Err("GATEWAY_MAX_BODY_BYTES must be greater than zero".into());
        }

        Ok(Self { listen_addr, request_timeout_secs, max_request_body_bytes, routes })
    }
}

fn validate_prefix(prefix: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !prefix.starts_with('/') || prefix == "/" || prefix.ends_with('/') {
        return Err(format!("route prefix {prefix:?} must start with /, must not be /, and must not end with /").into());
    }
    if prefix.split('/').any(|part| part == "." || part == "..") {
        return Err(format!("route prefix {prefix:?} cannot contain . or .. path segments").into());
    }
    if prefix == "/healthz" || prefix.starts_with("/healthz/") {
        return Err("route prefixes cannot overlap the reserved /healthz endpoint".into());
    }
    Ok(())
}

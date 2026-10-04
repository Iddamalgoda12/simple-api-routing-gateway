mod config;
mod proxy;

use std::{net::SocketAddr, time::Duration};

use axum::{Router, routing::get};
use config::GatewayConfig;
use proxy::{AppState, proxy_request};
use tokio::net::TcpListener;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let config = GatewayConfig::from_env()?;
    let listen_addr: SocketAddr = config.listen_addr.parse()?;
    let state = AppState::new(config.routes, Duration::from_secs(config.request_timeout_secs))?;

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .fallback(proxy_request)
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(config.max_request_body_bytes))
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!(%listen_addr, "gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}

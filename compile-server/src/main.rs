//! `windify-compile-server` — Remote Arduino/ESP32-S3 compile server.
//!
//! Accepts code payloads from mobile browsers that cannot run the desktop
//! Link Daemon, compiles them server-side using `arduino-cli`, and serves
//! the resulting flash binaries back so the browser can flash via USB OTG
//! (Web Serial / WebUSB + esptool-js).

mod api;
mod config;
mod job;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::job::JobRegistry;

/// Shared application state passed into every Axum handler.
pub struct AppState {
    pub config: Config,
    pub jobs: JobRegistry,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("windify_compile_server=info,tower_http=info")),
        )
        .init();

    let config = Config::from_env();
    let bind = format!("{}:{}", config.host, config.port);

    tracing::info!("[compile-server] starting on http://{}", bind);
    tracing::info!(
        "[compile-server] arduino-cli = {}",
        config.arduino_cli_path.display()
    );
    tracing::info!(
        "[compile-server] max_concurrent_jobs = {}",
        config.max_concurrent_jobs
    );

    let state = Arc::new(AppState {
        jobs: JobRegistry::new(config.clone()),
        config: config.clone(),
    });

    // Background job-expiry reaper.
    {
        let state2 = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                state2.jobs.reap_expired();
            }
        });
    }

    let cors = build_cors(&config.cors_origins);

    let app = Router::new()
        // REST compile endpoint.
        .route("/api/compile", post(api::compile::handle_compile))
        // WebSocket progress stream per job.
        .route("/job/:id/progress", get(api::progress::handle_progress))
        // Binary download per job.
        .route(
            "/api/compile/:id/download",
            get(api::download::handle_download),
        )
        .route(
            "/api/compile/:id/files/:filename",
            get(api::download::handle_download_file),
        )
        .route(
            "/api/compile/:id/zip",
            get(api::download::handle_download_zip),
        )
        // Health / identity.
        .route("/", get(api::health::handle_health))
        .route("/status", get(api::health::handle_status))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {}", bind, e));

    tracing::info!("[compile-server] listening on http://{}", bind);
    axum::serve(listener, app)
        .await
        .expect("server error");
}

fn build_cors(origins: &[String]) -> CorsLayer {
    if origins.is_empty() || origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        use tower_http::cors::AllowOrigin;
        let parsed: Vec<axum::http::HeaderValue> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_methods(Any)
            .allow_headers(Any)
    }
}

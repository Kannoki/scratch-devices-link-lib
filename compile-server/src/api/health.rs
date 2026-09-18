//! Health and status check endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::AppState;

pub const SERVER_NAME: &str = "windify-compile-server";

pub async fn handle_health() -> impl IntoResponse {
    SERVER_NAME
}

pub async fn handle_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cli_exists = state.config.arduino_cli_path.exists();

    Json(json!({
        "server": SERVER_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "ready": cli_exists,
        "arduinoCli": {
            "path": state.config.arduino_cli_path.display().to_string(),
            "found": cli_exists
        },
        "maxConcurrentJobs": state.config.max_concurrent_jobs,
        "jobTimeoutSecs": state.config.job_timeout_secs
    }))
}

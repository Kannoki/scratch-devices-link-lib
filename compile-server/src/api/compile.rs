//! Handler for `POST /api/compile`.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::job::worker::CompilePayload;
use crate::AppState;

/// Submits a compilation request.
/// Returns 202 Accepted with the newly generated `jobId` and streaming URLs.
pub async fn handle_compile(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CompilePayload>,
) -> impl IntoResponse {
    let (job_id, _rx) = state.jobs.enqueue(payload);

    let base_host = if state.config.host == "0.0.0.0" {
        "localhost"
    } else {
        &state.config.host
    };

    let ws_path = format!("/job/{}/progress", job_id);
    let ws_url = format!("ws://{}:{}/job/{}/progress", base_host, state.config.port, job_id);
    let download_url = format!("/api/compile/{}/download", job_id);

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "success": true,
            "jobId": job_id.to_string(),
            "wsPath": ws_path,
            "wsUrl": ws_url,
            "downloadUrl": download_url,
            "message": "Compilation enqueued"
        })),
    )
}

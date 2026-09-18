//! WebSocket handler for streaming compilation progress.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use uuid::Uuid;

use crate::job::JobState;
use crate::AppState;

/// Upgrades HTTP request to WebSocket to stream progress events for `job_id`.
pub async fn handle_progress(
    ws: WebSocketUpgrade,
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let job_uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let receiver = match state.jobs.subscribe(&job_uuid) {
        Some(rx) => rx,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let initial_state = state.jobs.state(&job_uuid);

    ws.on_upgrade(move |socket| stream_events(socket, receiver, initial_state))
        .into_response()
}

async fn stream_events(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<crate::job::ProgressEvent>,
    initial_state: Option<JobState>,
) {
    // If the job already finished before WebSocket connected, send immediate termination event.
    if let Some(state) = initial_state {
        match state {
            JobState::Done { download_url, .. } => {
                let success_msg = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "uploadSuccess",
                    "params": {
                        "aborted": false,
                        "downloadUrl": download_url
                    }
                });
                let _ = socket.send(Message::Text(success_msg.to_string())).await;
                let _ = socket.close().await;
                return;
            }
            JobState::Failed { message } => {
                let err_msg = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "uploadError",
                    "params": {
                        "message": message
                    }
                });
                let _ = socket.send(Message::Text(err_msg.to_string())).await;
                let _ = socket.close().await;
                return;
            }
            _ => {}
        }
    }

    // Loop receiving live broadcast events from the worker.
    while let Ok(event) = rx.recv().await {
        let is_terminal = event.method == "uploadSuccess" || event.method == "uploadError";
        let rpc = serde_json::json!({
            "jsonrpc": "2.0",
            "method": event.method,
            "params": event.params
        });

        if socket.send(Message::Text(rpc.to_string())).await.is_err() {
            break;
        }

        if is_terminal {
            break;
        }
    }

    let _ = socket.close().await;
}

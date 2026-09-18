//! Handlers for downloading compiled binary artifacts and manifest metadata.

use std::fs;
use std::io::Cursor;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::job::JobState;
use crate::AppState;

/// Main download handler: returns the `manifest.json` metadata containing flash offsets
/// and relative URLs for each binary component.
pub async fn handle_download(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let job_uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
    };

    let artifact_dir = match state.jobs.state(&job_uuid) {
        Some(JobState::Done { artifact_dir, .. }) => artifact_dir,
        Some(JobState::Failed { message }) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Job failed: {}", message),
            )
                .into_response();
        }
        Some(_) => {
            return (StatusCode::ACCEPTED, "Job is still in progress").into_response();
        }
        None => return (StatusCode::NOT_FOUND, "Job not found").into_response(),
    };

    let manifest_path = artifact_dir.join("manifest.json");
    if !manifest_path.exists() {
        return (StatusCode::NOT_FOUND, "Manifest not found").into_response();
    }

    match fs::read_to_string(&manifest_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(val) => (StatusCode::OK, Json(val)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid manifest json: {}", e),
            )
                .into_response(),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read manifest: {}", e),
        )
            .into_response(),
    }
}

/// Downloads a specific binary partition (e.g. `bootloader.bin`, `partitions.bin`, `app.bin`).
pub async fn handle_download_file(
    Path((id, filename)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let job_uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
    };

    let artifact_dir = match state.jobs.state(&job_uuid) {
        Some(JobState::Done { artifact_dir, .. }) => artifact_dir,
        _ => return (StatusCode::NOT_FOUND, "Artifact not found").into_response(),
    };

    // Sanitize filename to prevent directory traversal.
    let safe_name = match std::path::Path::new(&filename).file_name().and_then(|n| n.to_str()) {
        Some(n) if n == filename => n,
        _ => return (StatusCode::BAD_REQUEST, "Invalid file name").into_response(),
    };

    let file_path = artifact_dir.join(safe_name);
    if !file_path.exists() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    match fs::read(&file_path) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(
                CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", safe_name))
                    .unwrap(),
            );
            (StatusCode::OK, headers, Body::from(bytes)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Read error: {}", e),
        )
            .into_response(),
    }
}

/// Packages all artifacts into a zip file on the fly.
pub async fn handle_download_zip(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let job_uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
    };

    let artifact_dir = match state.jobs.state(&job_uuid) {
        Some(JobState::Done { artifact_dir, .. }) => artifact_dir,
        _ => return (StatusCode::NOT_FOUND, "Artifact not found").into_response(),
    };

    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buf));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        if let Ok(entries) = fs::read_dir(&artifact_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() {
                    let fname = p.file_name().unwrap().to_string_lossy();
                    if let Ok(bytes) = fs::read(&p) {
                        let _ = zip.start_file(fname.to_string(), options);
                        use std::io::Write;
                        let _ = zip.write_all(&bytes);
                    }
                }
            }
        }
        let _ = zip.finish();
    }

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"firmware_{}.zip\"", id)).unwrap(),
    );

    (StatusCode::OK, headers, Body::from(buf)).into_response()
}

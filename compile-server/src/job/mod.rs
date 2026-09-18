//! Job lifecycle: registry, state machine, and background worker launcher.

pub mod worker;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::Config;

/// All possible states a compile job can be in.
#[derive(Debug, Clone)]
pub enum JobState {
    /// Waiting for a worker slot.
    Queued,
    /// Currently compiling.
    Compiling,
    /// Compilation succeeded; artifacts stored at the given directory path.
    Done {
        /// Directory containing compiled binaries and `flash_args.json`.
        artifact_dir: std::path::PathBuf,
        /// Download URL suffix emitted to the client.
        download_url: String,
    },
    /// Compilation failed.
    Failed { message: String },
}

/// JSON-RPC-style progress notification sent over the WebSocket stream.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressEvent {
    pub method: String,
    pub params: serde_json::Value,
}

impl ProgressEvent {
    pub fn stdout(message: &str, progress: Option<f64>) -> Self {
        let mut params = serde_json::json!({ "message": message });
        if let Some(p) = progress {
            params["progress"] = serde_json::json!(p);
        }
        Self {
            method: "uploadStdout".to_string(),
            params,
        }
    }

    pub fn success(download_url: &str) -> Self {
        Self {
            method: "uploadSuccess".to_string(),
            params: serde_json::json!({ "downloadUrl": download_url }),
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            method: "uploadError".to_string(),
            params: serde_json::json!({ "message": message }),
        }
    }
}

/// A compile job entry in the registry.
#[allow(dead_code)]
pub struct Job {
    pub id: Uuid,
    pub state: JobState,
    /// Channel for streaming progress to WebSocket subscribers.
    pub progress_tx: broadcast::Sender<ProgressEvent>,
    /// When the job was created (for expiry reaping).
    pub created_at: Instant,
    /// When the job finished (for retain-period expiry).
    pub finished_at: Option<Instant>,
}

impl Job {
    fn new(id: Uuid) -> (Self, broadcast::Receiver<ProgressEvent>) {
        let (tx, rx) = broadcast::channel(256);
        (
            Self {
                id,
                state: JobState::Queued,
                progress_tx: tx,
                created_at: Instant::now(),
                finished_at: None,
            },
            rx,
        )
    }
}

/// Thread-safe job store.
pub struct JobRegistry {
    inner: Arc<DashMap<Uuid, Job>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    config: Config,
}

impl JobRegistry {
    pub fn new(config: Config) -> Self {
        let sem = tokio::sync::Semaphore::new(config.max_concurrent_jobs);
        Self {
            inner: Arc::new(DashMap::new()),
            semaphore: Arc::new(sem),
            config,
        }
    }

    /// Create a new job, enqueue it, and return the job ID and a receiver for
    /// subscribing to progress events.
    pub fn enqueue(
        &self,
        payload: worker::CompilePayload,
    ) -> (Uuid, broadcast::Receiver<ProgressEvent>) {
        let id = Uuid::new_v4();
        let (job, rx) = Job::new(id);
        self.inner.insert(id, job);

        // Clone handles for the worker task.
        let map = self.inner.clone();
        let sem = self.semaphore.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            // Acquire a concurrency slot — queues when at capacity.
            let _permit = sem.acquire().await.expect("semaphore closed");

            // Mark as Compiling.
            if let Some(mut entry) = map.get_mut(&id) {
                entry.state = JobState::Compiling;
            }

            let timeout = Duration::from_secs(config.job_timeout_secs);
            let result = tokio::time::timeout(
                timeout,
                worker::run_compile(id, &config, payload, map.clone()),
            )
            .await;

            match result {
                Ok(Ok((artifact_dir, download_url))) => {
                    if let Some(mut entry) = map.get_mut(&id) {
                        entry.state = JobState::Done { artifact_dir, download_url: download_url.clone() };
                        entry.finished_at = Some(Instant::now());
                        let _ = entry.progress_tx.send(ProgressEvent::success(&download_url));
                    }
                }
                Ok(Err(msg)) => {
                    if let Some(mut entry) = map.get_mut(&id) {
                        let _ = entry.progress_tx.send(ProgressEvent::error(&msg));
                        entry.state = JobState::Failed { message: msg };
                        entry.finished_at = Some(Instant::now());
                    }
                }
                Err(_elapsed) => {
                    let msg = format!("Compile job timed out after {} s", config.job_timeout_secs);
                    if let Some(mut entry) = map.get_mut(&id) {
                        let _ = entry.progress_tx.send(ProgressEvent::error(&msg));
                        entry.state = JobState::Failed { message: msg };
                        entry.finished_at = Some(Instant::now());
                    }
                }
            }
        });

        (id, rx)
    }

    /// Subscribe to progress events for an existing job.
    /// Returns `None` if the job doesn't exist.
    pub fn subscribe(&self, id: &Uuid) -> Option<broadcast::Receiver<ProgressEvent>> {
        self.inner.get(id).map(|e| e.progress_tx.subscribe())
    }

    /// Get current job state (cloned, non-blocking).
    pub fn state(&self, id: &Uuid) -> Option<JobState> {
        self.inner.get(id).map(|e| e.state.clone())
    }

    /// Remove all jobs that have been finished longer than `job_retain_secs`.
    pub fn reap_expired(&self) {
        let retain = Duration::from_secs(self.config.job_retain_secs);
        self.inner.retain(|_, job| {
            if let Some(finished) = job.finished_at {
                if finished.elapsed() > retain {
                    // Clean up artifact dir if present.
                    if let JobState::Done { ref artifact_dir, .. } = job.state {
                        let _ = std::fs::remove_dir_all(artifact_dir);
                    }
                    return false; // remove from map
                }
            }
            true
        });
    }
}

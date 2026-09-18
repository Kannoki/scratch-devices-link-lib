//! Server configuration, loaded from environment variables at startup.

use std::path::PathBuf;

/// All tunable server parameters.
#[derive(Clone, Debug)]
pub struct Config {
    /// TCP bind address (default: `0.0.0.0`).
    pub host: String,
    /// TCP port (default: `8765`).
    pub port: u16,
    /// Path to the `arduino-cli` binary.
    pub arduino_cli_path: PathBuf,
    /// Optional extra directories to search for Arduino libraries
    /// (colon-separated on Unix, semicolon-separated on Windows).
    pub extra_library_paths: Vec<PathBuf>,
    /// Optional path to a custom `arduino-cli.yaml` config file.
    pub arduino_config_file: Option<PathBuf>,
    /// Maximum number of compile jobs that may run concurrently.
    pub max_concurrent_jobs: usize,
    /// Hard timeout per compile job, in seconds.
    pub job_timeout_secs: u64,
    /// How long finished / failed jobs are retained before being reaped.
    pub job_retain_secs: u64,
    /// Allowed CORS origins (empty or `["*"]` → wildcard).
    pub cors_origins: Vec<String>,
}

impl Config {
    /// Read configuration from environment variables, applying defaults.
    pub fn from_env() -> Self {
        let host = std::env::var("COMPILE_SERVER_HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = std::env::var("COMPILE_SERVER_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8765u16);

        let arduino_cli_path = std::env::var("ARDUINO_CLI_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("arduino-cli"));

        let extra_library_paths = std::env::var("EXTRA_LIBRARY_PATH")
            .unwrap_or_default()
            .split(if cfg!(windows) { ';' } else { ':' })
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect();

        let arduino_config_file = std::env::var("ARDUINO_CONFIG_FILE")
            .ok()
            .map(PathBuf::from);

        let max_concurrent_jobs = std::env::var("MAX_CONCURRENT_JOBS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4usize);

        let job_timeout_secs = std::env::var("JOB_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(180u64);

        let job_retain_secs = std::env::var("JOB_RETAIN_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600u64);

        let cors_origins = std::env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "*".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            host,
            port,
            arduino_cli_path,
            extra_library_paths,
            arduino_config_file,
            max_concurrent_jobs,
            job_timeout_secs,
            job_retain_secs,
            cors_origins,
        }
    }
}

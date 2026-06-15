    //! Release-based toolchain management.
//!
//! Instead of downloading individual components from the Arduino/Espressif CDNs,
//! this module fetches pre-built, pre-pruned `.7z` archives from GitHub releases
//! and extracts them into the tools directory.  This eliminates the need to run
//! `arduino-cli core install` at runtime and gives us a single, versioned artifact
//! per platform per release.
//!
//! Update detection
//! -----------------
//! After a successful extraction we write `Arduino/.release-meta` — a small JSON
//! file recording the tag and `published_at` timestamp of the GitHub release.
//! On startup we compare that timestamp against the most-recent file modification
//! time inside `tools/Arduino/`.  If a newer release exists on GitHub, the app
//! re-downloads and replaces the tools.
//!
//! Release assets
//! ---------------
//! Expected asset name pattern (matching the CI archive script):
//!   `tools-pruned-{platform}-{arch}.7z`
//!   e.g. `tools-pruned-win32-x64.7z`, `tools-pruned-darwin-arm64.7z`
//!
//! A companion checksum file is expected alongside it:
//!   `tools-pruned-{platform}-{arch}-checksums-sha256.txt`

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;
use sha2::Digest;

// Shared types (SetupProgress, ProgressFn) live in a separate module.
pub use crate::toolchain_types::{ProgressFn, SetupProgress};

// ── Constants ────────────────────────────────────────────────────────────────

/// GitHub repo that publishes the tools archives.
/// Override with the `WINBLOCK_TOOLS_USER` env var.
fn tools_owner() -> String {
    std::env::var("WINBLOCK_TOOLS_USER").unwrap_or_else(|_| "winblockcc".to_string())
}

const TOOLS_REPO: &str = "winblock-tools";

/// Asset name pattern used by the CI archive script (no file extension).
fn archive_stem(platform: &str, arch: &str) -> String {
    format!("tools-pruned-{}-{}", platform, arch)
}

/// Returns the GitHub API URL for the latest release of `tools_repo`.
fn latest_release_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{}/{}/releases/latest", owner, repo)
}

// ── JSON models ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    published_at: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize, Clone)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize, serde::Serialize)]
struct ReleaseMeta {
    tag: String,
    published_at: String,
    platform: String,
}

// ── Platform / arch detection ───────────────────────────────────────────────

fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("reqwest client")
}

/// Write a JSON file, creating parent directories as needed.
fn write_json<P: AsRef<Path>, T: serde::Serialize>(path: P, value: &T) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Read a JSON file, returning None if it doesn't exist or is invalid.
fn read_json<P: AsRef<Path>, T: for<'de> Deserialize<'de>>(path: P) -> Option<T> {
    let bytes = fs::read(path.as_ref()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ── Release metadata ────────────────────────────────────────────────────────

/// Path inside the tools root where we persist release metadata.
fn release_meta_path(tools_path: &Path) -> PathBuf {
    tools_path.join("Arduino").join(".release-meta")
}

/// Load the persisted `.release-meta` file, if it exists.
fn load_meta(tools_path: &Path) -> Option<ReleaseMeta> {
    read_json(release_meta_path(tools_path))
}

/// Save the `.release-meta` file after a successful extraction.
fn save_meta(tools_path: &Path, tag: &str, published_at: &str) -> Result<(), String> {
    let meta = ReleaseMeta {
        tag: tag.to_string(),
        published_at: published_at.to_string(),
        platform: format!("{}-{}", current_platform(), current_arch()),
    };
    write_json(release_meta_path(tools_path), &meta)
}

// ── Minimal inline directory walker ──────────────────────────────────────────

/// Walk `root` recursively and call `f` for every file.
/// Avoids adding a `walkdir` crate dep — uses only std.
fn walk_files<F>(root: &Path, f: &mut F)
where
    F: FnMut(&Path),
{
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let file_type = entry.file_type().ok();
                if file_type.as_ref().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(entry.path());
                } else if file_type.as_ref().map(|t| t.is_file()).unwrap_or(false) {
                    f(&entry.path());
                }
            }
        }
    }
}

// ── Timestamp parsing ────────────────────────────────────────────────────────

/// Parse an RFC3339 / ISO8601 timestamp and return seconds since UNIX epoch.
/// Returns None if the string cannot be parsed.
fn parse_timestamp_to_epoch(s: &str) -> Option<i64> {
    // Format: 2026-06-10T12:00:00Z  (may have sub-second or timezone offset)
    // Strip the trailing Z / UTC marker.
    let s = s.trim_end_matches('Z').trim_end_matches('z').trim();
    let parts: Vec<&str> = s
        .split(|c| c == '-' || c == ':' || c == 'T' || c == '+')
        .collect();

    if parts.len() < 6 {
        return None;
    }

    let y: i64 = parts[0].parse().ok()?;
    let mo: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    let h: i64 = parts[3].parse().ok()?;
    let mi: i64 = parts[4].parse().ok()?;
    let sec: i64 = parts[5].parse().ok()?;

    // Approximate days since UNIX epoch using Julian day offset.
    // Y/M/D → ordinal days, then add h/mi/sec.
    let ym: i64 = if mo <= 2 { y - 1 } else { y };
    let julian: i64 = (1461 * ym) / 4
        + (1534 * (mo + if mo > 2 { 1 } else { 9 }) - 2) / 5
        + d
        - 32045;
    let days_since_epoch = julian - 2440588; // Julian day of UNIX epoch: 1970-01-01

    let secs = days_since_epoch * 86400 + h * 3600 + mi * 60 + sec;
    Some(secs)
}

// ── Update detection ─────────────────────────────────────────────────────────

/// Check whether a newer tools release exists on GitHub than what is on disk.
///
/// Strategy:
/// 1. Load `tools/Arduino/.release-meta` → local tag + published_at.
/// 2. Fetch latest GitHub release → remote tag + published_at.
/// 3. Compare parsed timestamps.
pub async fn needs_update(tools_path: &Path) -> Result<bool, String> {
    // No meta file → first install.
    let Some(meta) = load_meta(tools_path) else {
        return Ok(true);
    };

    let owner = tools_owner();
    let url = latest_release_url(&owner, TOOLS_REPO);

    let client = http_client();
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "future-academy-tray")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        tracing::warn!(
            "[link] could not check for tools update (HTTP {}): not forcing update",
            resp.status()
        );
        return Ok(false);
    }

    let remote: GithubRelease = resp.json().await.map_err(|e| e.to_string())?;

    let remote_secs = parse_timestamp_to_epoch(&remote.published_at);
    let local_secs = parse_timestamp_to_epoch(&meta.published_at);

    match (remote_secs, local_secs) {
        (Some(remote_t), Some(local_t)) => {
            let newer = remote_t > local_t;
            tracing::info!(
                "[link] tools update check: local={} remote={} → needs_update={}",
                meta.published_at,
                remote.published_at,
                newer
            );
            Ok(newer)
        }
        // Can't parse timestamps → compare tag strings.
        _ => {
            let tag_changed = remote.tag_name != meta.tag;
            tracing::info!(
                "[link] tools update check: tag changed {} → needs_update={}",
                tag_changed,
                tag_changed
            );
            Ok(tag_changed)
        }
    }
}

// ── Download helpers ────────────────────────────────────────────────────────

/// Stream-download `url` to `dest`, calling `on_progress(0-100)` as data arrives.
async fn download_file<F: FnMut(u8)>(
    url: &str,
    dest: &Path,
    mut on_progress: F,
) -> Result<(), String> {
    let resp = http_client()
        .get(url)
        .header("Accept", "application/octet-stream")
        .header("User-Agent", "future-academy-tray")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut received: u64 = 0;
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        received += chunk.len() as u64;
        if total > 0 {
            let pct = ((received as f64 / total as f64) * 100.0).round() as u8;
            on_progress(pct.min(100));
        }
    }

    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Fetch the GitHub releases API and return the matching `GithubRelease`.
async fn fetch_latest_release(owner: &str, repo: &str) -> Result<GithubRelease, String> {
    let url = latest_release_url(owner, repo);
    let client = http_client();
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "future-academy-tray")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;

    resp.json().await.map_err(|e| e.to_string())
}

/// Find the `.7z` archive asset matching the current platform + arch.
fn find_archive_asset<'a>(
    assets: &'a [GithubAsset],
    stem: &str,
) -> Option<&'a GithubAsset> {
    assets.iter().find(|a| a.name == format!("{}.7z", stem))
}

/// Find the checksum asset matching the archive stem.
fn find_checksum_asset<'a>(
    assets: &'a [GithubAsset],
    stem: &str,
) -> Option<&'a GithubAsset> {
    assets
        .iter()
        .find(|a| a.name == format!("{}-checksums-sha256.txt", stem))
}

/// Verify the SHA-256 checksum of `file` matches the hex string in `checksum_file`.
fn verify_checksum(file: &Path, checksum_file: &Path) -> Result<(), String> {
    let checksum_data = fs::read_to_string(checksum_file).map_err(|e| e.to_string())?;
    let expected = checksum_data.split_whitespace().next()
        .ok_or("checksum file is empty")?;

    let mut reader = fs::File::open(file).map_err(|e| e.to_string())?;
    let mut context = sha2::Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        context.update(&buf[..n]);
    }
    let digest = format!("{:x}", context.finalize());

    if digest == expected {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch: expected {}, got {}",
            expected, digest
        ))
    }
}

// ── main public API ──────────────────────────────────────────────────────────

/// Download the latest pre-built tools archive from GitHub releases,
/// verify its checksum, extract it into `tools_path`, and write `.release-meta`.
pub async fn ensure_from_release(
    tools_path: &Path,
    report: ProgressFn,
) -> Result<(), String> {
    let owner = tools_owner();
    let platform = current_platform();
    let arch = current_arch();
    let stem = archive_stem(platform, arch);

    let set_phase = |report: &ProgressFn, phase_name: &str, progress: u8| {
        report(SetupProgress {
            phase: phase_name.to_string(),
            progress,
        });
    };

    // 1. Fetch latest GitHub release.
    set_phase(&report, "checking-for-updates", 0);
    let release = fetch_latest_release(&owner, TOOLS_REPO).await?;

    // 2. Find the matching archive + checksum assets.
    let archive_asset = find_archive_asset(&release.assets, &stem)
        .ok_or_else(|| {
            format!(
                "no archive asset found for {}-{} in release {}",
                platform, arch, release.tag_name
            )
        })?
        .clone();

    let checksum_asset = find_checksum_asset(&release.assets, &stem).cloned();

    set_phase(&report, "downloading-tools", 0);

    // 3. Download to a temp directory inside tools_path.
    let tmp_dir = tools_path.join(".setup-tmp");
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let archive_path = tmp_dir.join(&archive_asset.name);
    let checksum_path = tmp_dir.join(format!("{}-checksums-sha256.txt", &stem));

    {
        let report_dl = report.clone();
        download_file(&archive_asset.browser_download_url, &archive_path, move |pct| {
            report_dl(SetupProgress {
                phase: "downloading-tools".to_string(),
                progress: pct,
            });
        })
        .await?;
    }

    // 4. Download and verify checksum (best-effort).
    if let Some(cs) = checksum_asset {
        set_phase(&report, "verifying-checksum", 0);
        let report_cs = report.clone();
        download_file(&cs.browser_download_url, &checksum_path, move |_| {
            report_cs(SetupProgress {
                phase: "verifying-checksum".to_string(),
                progress: 0,
            });
        })
        .await?;
        verify_checksum(&archive_path, &checksum_path)?;
    }

    // 5. Extract into `tools_path` using toolchain.rs's existing helpers.
    set_phase(&report, "extracting-tools", 0);
    let report_ext = report.clone();
    let on_extract_progress: Arc<dyn Fn(u8) + Send + Sync> = Arc::new(move |pct: u8| {
        report_ext(SetupProgress {
            phase: "extracting-tools".to_string(),
            progress: pct,
        });
    });

    let archive_path_for_blocking = archive_path.clone();
    let tools_path_for_blocking = tools_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        crate::toolchain::extract_archive_stripped(
            &archive_path_for_blocking,
            &tools_path_for_blocking,
            Some(&on_extract_progress),
        )
    })
    .await
    .map_err(|e| e.to_string())??
    ;

    // 6. Write release metadata.
    save_meta(tools_path, &release.tag_name, &release.published_at)?;

    // 7. Clean up temp files.
    let _ = fs::remove_file(&archive_path);
    let _ = fs::remove_file(&checksum_path);
    let _ = fs::remove_dir_all(&tmp_dir);

    set_phase(&report, "done", 100);
    Ok(())
}

//! Atomic runtime installation of the platform tool package.
//!
//! The archive is downloaded and extracted beside the destination using only
//! Rust APIs. The staged package is permission-repaired and fully validated
//! before it replaces an existing install, which keeps retries safe and avoids
//! shell quoting/code-page problems on Windows.

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(target_os = "macos")]
pub const TOOLS_7Z: &str = "tools-mac.7z";
#[cfg(not(target_os = "macos"))]
pub const TOOLS_7Z: &str = "tools.7z";

pub const ASSET_BASE: &str =
    "https://github.com/Kannoki/scratch-devices-link-lib/releases/download/Tools/";

const DOWNLOAD_ATTEMPTS: usize = 5;
const DOWNLOAD_READ_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

pub type ProgressFn = Arc<dyn Fn(u8) + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsStatus {
    Present,
    Downloaded,
    Failed(String),
}

fn acquire_install_lock(tools_path: &Path) -> Result<File, String> {
    let parent = tools_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create tools parent {}: {error}", parent.display()))?;
    let lock_path = parent.join(".windy-tools-install.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("open tool installer lock {}: {error}", lock_path.display()))?;

    tracing::info!("[tools] waiting for exclusive installer lock");
    file.lock()
        .map_err(|error| format!("lock tool installer {}: {error}", lock_path.display()))?;
    tracing::info!("[tools] acquired exclusive installer lock");
    Ok(file)
}

/// Validate an existing install or atomically replace it with a fresh package.
pub fn ensure_tools(tools_path: &Path, progress: ProgressFn) -> ToolsStatus {
    match crate::toolchain::repair_executable_permissions(tools_path) {
        Ok(repaired) if repaired > 0 => {
            tracing::info!("[tools] restored execute permission on {repaired} files");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!("[tools] permission repair failed: {error}"),
    }

    let current = crate::toolchain::validate_toolchain(tools_path);
    if current.is_ready() {
        progress(100);
        return ToolsStatus::Present;
    }
    if tools_path.exists() {
        tracing::warn!(
            "[tools] existing package is incomplete: {}",
            current.missing.join("; ")
        );
    }

    // Another process may be installing the same shared package. Hold an OS
    // lock for the whole download/extract/activation transaction, then
    // revalidate because the process ahead of us may have completed it.
    let _install_lock = match acquire_install_lock(tools_path) {
        Ok(lock) => lock,
        Err(error) => {
            return ToolsStatus::Failed(format!("tool package installation failed: {error}"))
        }
    };
    if let Err(error) = crate::toolchain::repair_executable_permissions(tools_path) {
        tracing::warn!("[tools] post-lock permission repair failed: {error}");
    }
    if crate::toolchain::validate_toolchain(tools_path).is_ready() {
        tracing::info!("[tools] another process completed toolchain installation");
        progress(100);
        return ToolsStatus::Present;
    }

    tracing::info!(
        "[tools] installing {} into {}",
        TOOLS_7Z,
        tools_path.display()
    );
    match download_extract_and_install(tools_path, progress) {
        Ok(()) => ToolsStatus::Downloaded,
        Err(error) => ToolsStatus::Failed(format!("tool package installation failed: {error}")),
    }
}

fn download_extract_and_install(tools_path: &Path, progress: ProgressFn) -> Result<(), String> {
    let parent = tools_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create tools parent {}: {error}", parent.display()))?;

    let id = Uuid::new_v4().simple().to_string();
    let archive = parent.join(format!(".windy-download-{TOOLS_7Z}.partial"));
    let stage = parent.join(format!(".windy-tools-{id}.stage"));
    let url = format!("{ASSET_BASE}{TOOLS_7Z}");

    cleanup_interrupted_install_stages(parent, &archive);

    // Return early on network failure so the stable partial archive remains
    // available for Range-resume on the next app launch.
    download_with_retries(&url, &archive, progress.clone())?;

    let result = (|| {
        fs::create_dir(&stage)
            .map_err(|error| format!("create extraction stage {}: {error}", stage.display()))?;
        // sevenz-rust2 has historically panicked inside Windows path APIs for
        // some Unicode/junction paths. Convert that panic into a normal setup
        // failure so the previous atomic install remains usable.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sevenz_rust2::decompress_file(&archive, &stage)
        }))
        .map_err(|panic| {
            let message = panic
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown sevenz-rust2 panic".to_string());
            format!("extract {TOOLS_7Z} panicked: {message}")
        })?
        .map_err(|error| format!("extract {TOOLS_7Z}: {error}"))?;

        let prepared = extracted_package_root(&stage)?;
        let repaired = crate::toolchain::repair_executable_permissions(&prepared)?;
        if repaired > 0 {
            tracing::info!("[tools] restored execute permission on {repaired} staged files");
        }
        let validation = crate::toolchain::validate_toolchain(&prepared);
        if !validation.is_ready() {
            return Err(format!(
                "downloaded archive is incomplete: {}",
                validation.missing.join("; ")
            ));
        }

        replace_atomically(&prepared, tools_path)?;
        progress(100);
        Ok(())
    })();

    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&stage);
    result
}

fn cleanup_interrupted_install_stages(parent: &Path, active_archive: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == active_archive {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with(".windy-tools-") && name.ends_with(".7z.partial") {
            if let Err(error) = fs::remove_file(&path) {
                tracing::warn!(
                    "[tools] could not remove legacy partial download {}: {error}",
                    path.display()
                );
            }
        } else if name.starts_with(".windy-tools-") && name.ends_with(".stage") {
            if let Err(error) = fs::remove_dir_all(&path) {
                tracing::warn!(
                    "[tools] could not remove interrupted extraction stage {}: {error}",
                    path.display()
                );
            }
        }
    }
}

fn download_with_retries(
    url: &str,
    destination: &Path,
    progress: ProgressFn,
) -> Result<(), String> {
    download_with_retries_and_delay(url, destination, progress, Duration::from_secs(2))
}

fn download_with_retries_and_delay(
    url: &str,
    destination: &Path,
    progress: ProgressFn,
    retry_delay: Duration,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        let existing = fs::metadata(destination)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if existing > 0 {
            tracing::info!(
                "[tools] download attempt {attempt}/{DOWNLOAD_ATTEMPTS}: resuming at byte {existing}"
            );
        } else {
            tracing::info!(
                "[tools] download attempt {attempt}/{DOWNLOAD_ATTEMPTS}: starting from byte 0"
            );
            progress(0);
        }
        match download_once(url, destination, progress.clone()) {
            Ok(()) => return Ok(()),
            Err(error) => {
                errors.push(format!("attempt {attempt}: {error}"));
                if attempt < DOWNLOAD_ATTEMPTS && !retry_delay.is_zero() {
                    std::thread::sleep(retry_delay);
                }
            }
        }
    }
    Err(format!(
        "download failed after {DOWNLOAD_ATTEMPTS} attempts ({})",
        errors.join(" | ")
    ))
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse().ok()?;
    let end = end.parse().ok()?;
    let total = total.parse().ok()?;
    if start > end || end >= total {
        return None;
    }
    Some((start, end, total))
}

fn sha256_file(path: &Path) -> Result<(u64, String), String> {
    let file = File::open(path)
        .map_err(|error| format!("open {} for hashing: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 256 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("hash {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}

fn download_once(url: &str, destination: &Path, progress: ProgressFn) -> Result<(), String> {
    let mut resume_offset = fs::metadata(destination)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(DOWNLOAD_CONNECT_TIMEOUT)
        .timeout_read(DOWNLOAD_READ_IDLE_TIMEOUT)
        .timeout_write(Duration::from_secs(90))
        .build();
    let mut request = agent.get(url).set("Accept-Encoding", "identity");
    if resume_offset > 0 {
        request = request.set("Range", &format!("bytes={resume_offset}-"));
    }
    let response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::Status(416, _)) if resume_offset > 0 => {
            fs::remove_file(destination).map_err(|error| {
                format!(
                    "reset rejected partial download {}: {error}",
                    destination.display()
                )
            })?;
            return Err("server rejected the saved byte range; partial download reset".to_string());
        }
        Err(error) => return Err(format!("GET {url}: {error}")),
    };

    let status = response.status();
    let (append, expected_size) = if status == 206 {
        let content_range = response
            .header("Content-Range")
            .and_then(parse_content_range)
            .ok_or_else(|| "range response is missing a valid Content-Range header".to_string())?;
        if content_range.0 != resume_offset {
            return Err(format!(
                "range response starts at {}, expected {resume_offset}",
                content_range.0
            ));
        }
        (true, Some(content_range.2))
    } else if status == 200 {
        // A server may ignore Range or reject If-Range after an asset change.
        // Restart safely rather than appending a full response to old bytes.
        resume_offset = 0;
        (
            false,
            response
                .header("Content-Length")
                .and_then(|value| value.parse::<u64>().ok()),
        )
    } else {
        return Err(format!("GET {url}: unexpected HTTP {status}"));
    };

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(destination)
        .map_err(|error| format!("open {} for download: {error}", destination.display()))?;
    let mut writer = BufWriter::new(file);
    let mut reader = response.into_reader();
    let mut received = resume_offset;
    let mut buffer = [0_u8; 256 * 1024];

    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) => {
                let _ = writer.flush();
                let _ = writer.get_ref().sync_all();
                return Err(format!(
                    "read response after {received} bytes: {error}; partial download kept"
                ));
            }
        };
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| format!("write {}: {error}", destination.display()))?;
        received += count as u64;
        if let Some(total) = expected_size.filter(|total| *total > 0) {
            let percent = ((received.saturating_mul(100)) / total).min(99) as u8;
            progress(percent);
        }
    }
    writer
        .flush()
        .map_err(|error| format!("flush {}: {error}", destination.display()))?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|error| format!("sync {}: {error}", destination.display()))?;

    if let Some(expected) = expected_size {
        if received != expected {
            return Err(format!(
                "truncated response: received {received} of {expected} bytes"
            ));
        }
    }
    if received == 0 {
        return Err("server returned an empty archive".to_string());
    }

    let (verified_size, sha256) = sha256_file(destination)?;
    if verified_size != received {
        return Err(format!(
            "download changed while verifying: expected {received} bytes, found {verified_size}"
        ));
    }
    tracing::info!("[tools] downloaded {received} bytes; sha256={sha256}");
    Ok(())
}

/// Accept either an archive with a single `tools/` wrapper or a flat archive.
fn extracted_package_root(stage: &Path) -> Result<PathBuf, String> {
    let entries: Vec<PathBuf> = fs::read_dir(stage)
        .map_err(|error| format!("read extraction stage {}: {error}", stage.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("read extracted entry: {error}"))
        })
        .collect::<Result<_, _>>()?;

    if entries.len() == 1 && entries[0].is_dir() {
        Ok(entries[0].clone())
    } else if entries.is_empty() {
        Err("archive extracted no files".to_string())
    } else {
        Ok(stage.to_path_buf())
    }
}

/// Replace `destination` only after `prepared` has passed validation.
///
/// Both paths are siblings on the same filesystem, so rename is atomic. If
/// activation fails, the previous package is restored.
fn replace_atomically(prepared: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let backup = parent.join(format!(".windy-tools-backup-{}", Uuid::new_v4().simple()));
    let had_previous = destination.exists();

    if had_previous {
        fs::rename(destination, &backup).map_err(|error| {
            format!(
                "move previous tools package {} aside: {error}",
                destination.display()
            )
        })?;
    }

    if let Err(error) = fs::rename(prepared, destination) {
        if had_previous {
            let _ = fs::rename(&backup, destination);
        }
        return Err(format!(
            "activate tools package at {}: {error}",
            destination.display()
        ));
    }

    if had_previous {
        if let Err(error) = fs::remove_dir_all(&backup) {
            tracing::warn!(
                "[tools] could not remove previous package {}: {error}",
                backup.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{label}-{}", Uuid::new_v4()))
    }

    #[test]
    fn detects_wrapped_and_flat_archives() {
        let wrapped = test_root("tools-wrapped");
        fs::create_dir_all(wrapped.join("tools/Arduino")).unwrap();
        assert_eq!(
            extracted_package_root(&wrapped).unwrap(),
            wrapped.join("tools")
        );

        let flat = test_root("tools-flat");
        fs::create_dir_all(flat.join("Arduino")).unwrap();
        fs::write(flat.join("manifest.json"), b"{}").unwrap();
        assert_eq!(extracted_package_root(&flat).unwrap(), flat);

        fs::remove_dir_all(wrapped).unwrap();
        fs::remove_dir_all(flat).unwrap();
    }

    #[test]
    fn atomically_replaces_existing_package_under_unicode_path() {
        let root = test_root("Công cụ");
        let destination = root.join("Thiết bị");
        let prepared = root.join("prepared");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&prepared).unwrap();
        fs::write(destination.join("version"), b"old").unwrap();
        fs::write(prepared.join("version"), b"new").unwrap();

        replace_atomically(&prepared, &destination).unwrap();
        assert_eq!(fs::read(destination.join("version")).unwrap(), b"new");
        assert!(!prepared.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_http_content_ranges() {
        assert_eq!(
            parse_content_range("bytes 5-9/10"),
            Some((5_u64, 9_u64, 10_u64))
        );
        assert_eq!(parse_content_range("bytes 10-9/10"), None);
        assert_eq!(parse_content_range("bytes 5-10/10"), None);
        assert_eq!(parse_content_range("invalid"), None);
    }

    #[test]
    fn resumes_a_truncated_http_download() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let count = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                if attempt == 0 {
                    assert!(!request.contains("Range:"));
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nhello",
                        )
                        .unwrap();
                } else {
                    assert!(request.to_ascii_lowercase().contains("range: bytes=5-"));
                    stream
                        .write_all(
                            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 5-9/10\r\nConnection: close\r\n\r\nworld",
                        )
                        .unwrap();
                }
            }
        });

        let root = test_root("tools-resume");
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("tools.7z.partial");
        let progress: ProgressFn = Arc::new(|_| {});
        download_with_retries_and_delay(
            &format!("http://{address}/tools.7z"),
            &destination,
            progress,
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"helloworld");

        server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn serializes_concurrent_tool_installers() {
        use std::sync::mpsc;

        let root = test_root("tools-lock");
        let tools_path = root.join("tools");
        let first = acquire_install_lock(&tools_path).unwrap();
        let (tx, rx) = mpsc::channel();
        let second_tools_path = tools_path.clone();
        let waiter = std::thread::spawn(move || {
            let second = acquire_install_lock(&second_tools_path);
            tx.send(second.is_ok()).unwrap();
        });

        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        drop(first);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), true);
        waiter.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

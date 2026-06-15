//! Archive extraction helpers (no download logic).
//!
//! This module provides pure extraction utilities for `.zip`, `.tar.gz`, and
//! `.tar.bz2` archives.  Runtime tool acquisition has moved to
//! `toolchain_release.rs`, which downloads pre-built `.7z` archives from
//! GitHub releases.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use bzip2::read::BzDecoder;

    // Shared types (SetupProgress, ProgressFn) are defined in toolchain_types.rs to
// avoid a circular dependency with toolchain_release.rs.
#[allow(unused_imports)]
pub use crate::toolchain_types::{ProgressFn, SetupProgress};

/// Detected archive format, based on magic bytes at the start of the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArchiveFormat {
    TarGz,
    TarBz2,
    Zip,
}

/// Read the first 2 bytes of the file and identify the compression format.
/// - `1f 8b` → gzip  (TarGz)
/// - `42 5a` → bzip2 (TarBz2)
/// - `50 4b` → zip   (Zip)
pub fn detect_format(path: &Path) -> Result<ArchiveFormat, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 2];
    f.read_exact(&mut header).map_err(|e| e.to_string())?;
    match header {
        [0x1f, 0x8b] => Ok(ArchiveFormat::TarGz),
        [0x42, 0x5a] => Ok(ArchiveFormat::TarBz2),
        [0x50, 0x4b] => Ok(ArchiveFormat::Zip),
        _ => Err(format!(
            "unsupported or corrupt archive (magic {:02x}{:02x}): {}",
            header[0], header[1],
            path.display()
        )),
    }
}

#[cfg(windows)]
pub const CLI_FILE: &str = "arduino-cli.exe";
#[cfg(not(windows))]
pub const CLI_FILE: &str = "arduino-cli";

/// Simple progress callback used during extraction, separate from the main
/// `SetupProgress` channel.
type ExtractProgress = dyn Fn(u8) + Send + Sync + 'static;

// ── Extraction helpers ────────────────────────────────────────────────────────

/// First path component of a relative path as a String, or empty if none.
pub fn first_component(p: &Path) -> String {
    p.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Given the first path component of every entry, return the single shared one
/// if (and only if) ALL entries share it, else None (strip nothing).
pub fn common_first_component(firsts: &[String]) -> Option<String> {
    let mut iter = firsts.iter().filter(|s| !s.is_empty());
    let first = iter.next()?.clone();
    if firsts.iter().all(|c| !c.is_empty() && *c == first) {
        Some(first)
    } else {
        None
    }
}

/// Extract a .tar.gz or .tar.bz2 archive into dest_dir, reporting per-entry progress.
fn extract_tar_compressed(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    let format = detect_format(archive_path)?;
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let decompressor: Box<dyn Read> = match format {
        ArchiveFormat::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveFormat::TarBz2 => Box::new(BzDecoder::new(file)),
        ArchiveFormat::Zip => unreachable!(),
    };
    let mut archive = tar::Archive::new(decompressor);

    let entries: Vec<_> = archive.entries().map_err(|e| e.to_string())?.collect();
    let total = entries.len();
    for (i, entry_result) in entries.into_iter().enumerate() {
        let mut entry = entry_result.map_err(|e| e.to_string())?;
        entry.unpack(dest_dir).map_err(|e| e.to_string())?;
        if let Some(cb) = on_progress {
            let pct = (((i + 1) as f64 / total as f64) * 100.0).round() as u8;
            cb(pct.min(100));
        }
    }
    Ok(())
}

/// Extract a .zip archive into dest_dir, reporting per-entry progress.
fn extract_zip(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let total = zip.len();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let out_path = match entry.enclosed_name() {
            Some(p) => dest_dir.join(p),
            None => continue,
        };
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        if let Some(cb) = on_progress {
            let pct = (((i + 1) as f64 / total as f64) * 100.0).round() as u8;
            cb(pct.min(100));
        }
    }
    Ok(())
}

/// Detect archive format and extract into dest_dir.
pub fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let format = detect_format(archive_path)?;
    match format {
        ArchiveFormat::TarGz | ArchiveFormat::TarBz2 => {
            extract_tar_compressed(archive_path, dest_dir, on_progress)
        }
        ArchiveFormat::Zip => extract_zip(archive_path, dest_dir, on_progress),
    }
}

fn extract_zip_stripped(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut firsts: Vec<String> = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if let Some(p) = entry.enclosed_name() {
            firsts.push(first_component(&p));
        }
    }
    let prefix = common_first_component(&firsts);

    let total = zip.len() as u64;
    let mut extracted: u64 = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let rel = match entry.enclosed_name() {
            Some(p) => p,
            None => continue,
        };
        let stripped: &Path = match &prefix {
            Some(pfx) => rel.strip_prefix(pfx).unwrap_or(&rel),
            None => &rel,
        };
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let out_path = dest_dir.join(stripped);
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        extracted += 1;
        if let Some(cb) = on_progress {
            let pct = ((extracted as f64 / total as f64) * 100.0).round() as u8;
            cb(pct.min(100));
        }
    }
    Ok(())
}

fn extract_tar_compressed_stripped(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    let format = detect_format(archive_path)?;

    // Pass 1: count entries and collect first components.
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let decompressor: Box<dyn Read> = match format {
        ArchiveFormat::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveFormat::TarBz2 => Box::new(BzDecoder::new(file)),
        ArchiveFormat::Zip => unreachable!(),
    };
    let mut archive = tar::Archive::new(decompressor);
    let mut firsts: Vec<String> = Vec::new();
    let mut entry_count: u64 = 0;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;
        firsts.push(first_component(&path));
        entry_count += 1;
    }
    let prefix = common_first_component(&firsts);

    // Pass 2: extract with prefix stripped.
    let file = fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let decompressor: Box<dyn Read> = match format {
        ArchiveFormat::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveFormat::TarBz2 => Box::new(BzDecoder::new(file)),
        ArchiveFormat::Zip => unreachable!(),
    };
    let mut archive = tar::Archive::new(decompressor);
    let mut extracted: u64 = 0;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let rel = entry.path().map_err(|e| e.to_string())?.into_owned();
        let stripped: &Path = match &prefix {
            Some(pfx) => rel.strip_prefix(pfx).unwrap_or(&rel),
            None => &rel,
        };
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let out_path = dest_dir.join(stripped);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry.unpack(&out_path).map_err(|e| e.to_string())?;
        extracted += 1;
        if let Some(cb) = on_progress {
            let pct = ((extracted as f64 / entry_count as f64) * 100.0).round() as u8;
            cb(pct.min(100));
        }
    }
    Ok(())
}

/// Extract a .zip or .tar.gz archive into dest_dir, stripping a common leading
/// directory component shared by ALL entries. Used by toolchain_release.rs.
pub fn extract_archive_stripped(
    archive_path: &Path,
    dest_dir: &Path,
    on_progress: Option<&Arc<ExtractProgress>>,
) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let format = detect_format(archive_path)?;
    match format {
        ArchiveFormat::TarGz | ArchiveFormat::TarBz2 => {
            extract_tar_compressed_stripped(archive_path, dest_dir, on_progress)
        }
        ArchiveFormat::Zip => extract_zip_stripped(archive_path, dest_dir, on_progress),
    }
}

/// Run arduino-cli with args + `--config-file`. Blocking.
pub fn run_cli(cli_path: &Path, args: &[&str], config_path: &Path) -> Result<(), String> {
    let mut cmd = Command::new(cli_path);
    cmd.args(args);
    cmd.arg("--config-file").arg(config_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "arduino-cli {} failed (exit {:?})",
            args.first().copied().unwrap_or(""),
            status.code()
        ));
    }
    Ok(())
}

// ── toolchain check ───────────────────────────────────────────────────────────

/// Check if arduino-cli exists under `tools_path/Arduino/`.
pub fn check_toolchain(tools_path: &Path) -> (bool, PathBuf) {
    let cli_path = tools_path.join("Arduino").join(CLI_FILE);
    (cli_path.exists(), cli_path)
}

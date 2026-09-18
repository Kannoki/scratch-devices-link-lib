//! Worker task that sets up sketch workspace and executes `arduino-cli compile`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::config::Config;
use crate::job::{Job, ProgressEvent};

/// Input payload sent by the client.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompilePayload {
    /// Version flag (typically 1).
    pub v: Option<i64>,
    /// Main sketch C++ code or raw text.
    pub main: Option<String>,
    /// Optional extra sketch files: filename -> content.
    pub files: Option<BTreeMap<String, String>>,
    /// Optional bundled libraries: LibName -> (filepath -> content).
    pub libraries: Option<BTreeMap<String, BTreeMap<String, String>>>,
    /// Build and target configuration.
    pub config: Option<Value>,
    /// If client sent a base64-encoded message or JSON string in `message`.
    pub message: Option<String>,
}

/// Metadata describing flash layout and generated binaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashManifest {
    pub target: String,
    pub fqbn: String,
    pub chip: String,
    pub flash_mode: String,
    pub flash_freq: String,
    pub flash_size: String,
    pub parts: Vec<FlashPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashPart {
    pub name: String,
    pub offset: u64,
    pub offset_hex: String,
    pub filename: String,
    pub size_bytes: u64,
}

impl CompilePayload {
    /// Parse and normalize payload (handles raw string, JSON, or base64 wrapper).
    pub fn normalize(mut self) -> Result<Self, String> {
        if let Some(msg) = self.message.take() {
            let decoded_str = if let Ok(bytes) = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                msg.trim(),
            ) {
                String::from_utf8(bytes).unwrap_or(msg)
            } else {
                msg
            };

            if decoded_str.trim_start().starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<CompilePayload>(&decoded_str) {
                    let mut res = parsed;
                    if res.config.is_none() {
                        res.config = self.config;
                    }
                    return Ok(res);
                }
            }

            if self.main.is_none() {
                self.main = Some(decoded_str);
            }
        }
        Ok(self)
    }
}

/// Run compilation workflow for a single job.
pub async fn run_compile(
    job_id: Uuid,
    config: &Config,
    raw_payload: CompilePayload,
    jobs: Arc<DashMap<Uuid, Job>>,
) -> Result<(PathBuf, String), String> {
    let payload = raw_payload.normalize()?;

    let send_progress = {
        let jobs = jobs.clone();
        move |msg: &str, p: Option<f64>| {
            if let Some(entry) = jobs.get(&job_id) {
                let _ = entry.progress_tx.send(ProgressEvent::stdout(msg, p));
            }
        }
    };

    send_progress("[compile-server] Preparing workspace...\n", Some(0.05));

    let fqbn = resolve_fqbn(&payload.config);
    if fqbn.is_empty() {
        return Err("Missing FQBN in compile config".to_string());
    }

    send_progress(&format!("[compile-server] Target FQBN: {}\n", fqbn), None);

    // Setup temporary directory for building.
    let temp_dir = tempfile::Builder::new()
        .prefix(&format!("windify_job_{}_", job_id))
        .tempdir()
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let sketch_dir = temp_dir.path().join("sketch");
    let build_dir = temp_dir.path().join("build");
    let cache_dir = temp_dir.path().join("cache");

    fs::create_dir_all(&sketch_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    // Extract main sketch & extra files.
    let raw_main = payload.main.unwrap_or_default();
    let (cleaned_main, extracted_extra) = extract_windify_extra_files(&raw_main);

    let mut all_files = extracted_extra;
    if let Some(files) = payload.files {
        for (k, v) in files {
            all_files.insert(k, v);
        }
    }

    // Write sketch main file (must match directory name or be named sketch.ino).
    let sketch_ino_path = sketch_dir.join("sketch.ino");
    fs::write(&sketch_ino_path, &cleaned_main).map_err(|e| e.to_string())?;

    // Write extra files (e.g. headers or cpp clips).
    for (name, content) in &all_files {
        let safe_name = Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !safe_name.is_empty() {
            fs::write(sketch_dir.join(safe_name), content).map_err(|e| e.to_string())?;
        }
    }

    // Handle ESP32 custom partitions if audio or 16MB flash.
    let is_esp32 = fqbn.to_lowercase().starts_with("esp32:");
    let is_16mb = fqbn.to_lowercase().contains("flashsize=16m");
    let has_audio = all_files
        .keys()
        .any(|k| k.starts_with("windify_audio_clip_") && k.ends_with(".cpp"));

    if is_esp32 && (has_audio || is_16mb) {
        let partitions_path = sketch_dir.join("partitions.csv");
        if has_audio {
            const WINDIFY_ESP32_16MB_PARTITIONS_CSV: &str = "# Name, Type, SubType, Offset, Size, Flags\n\
                nvs, data, nvs, 0x9000, 0x5000,\n\
                otadata, data, ota, 0xe000, 0x2000,\n\
                app0, app, ota_0, 0x10000, 0xC80000,\n\
                spiffs, data, spiffs, 0xC90000, 0x360000,\n\
                coredump, data, coredump, 0xFF0000, 0x10000,\n";
            let _ = fs::write(&partitions_path, WINDIFY_ESP32_16MB_PARTITIONS_CSV);
        }
    }

    // Write bundled libraries.
    let mut bundled_libs_path: Option<PathBuf> = None;
    if let Some(libs) = payload.libraries {
        if !libs.is_empty() {
            let libs_dir = temp_dir.path().join("libraries");
            fs::create_dir_all(&libs_dir).map_err(|e| e.to_string())?;
            for (lib_name, files) in libs {
                let lib_path = libs_dir.join(lib_name);
                for (file_rel, content) in files {
                    let dest = lib_path.join(file_rel);
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    fs::write(dest, content).map_err(|e| e.to_string())?;
                }
            }
            bundled_libs_path = Some(libs_dir);
        }
    }

    // Construct arduino-cli arguments.
    let mut args: Vec<OsString> = vec![
        "compile".into(),
        "--fqbn".into(),
        fqbn.clone().into(),
        "--warnings=none".into(),
        "--verbose".into(),
        "--build-path".into(),
        build_dir.as_os_str().to_owned(),
        "--build-cache-path".into(),
        cache_dir.as_os_str().to_owned(),
    ];

    if let Some(ref cfg_file) = config.arduino_config_file {
        if cfg_file.exists() {
            args.push("--config-file".into());
            args.push(cfg_file.as_os_str().to_owned());
        }
    }

    // Bundled libraries.
    if let Some(ref bl) = bundled_libs_path {
        args.push("--libraries".into());
        args.push(bl.as_os_str().to_owned());
    }

    // Extra search paths.
    for p in &config.extra_library_paths {
        if p.exists() {
            args.push("--libraries".into());
            args.push(p.as_os_str().to_owned());
        }
    }

    // Compiler defines if present.
    if let Some(cfg) = &payload.config {
        if let Some(defines) = cfg.get("compilerDefines").and_then(|v| v.as_array()) {
            let flags: Vec<String> = defines
                .iter()
                .filter_map(|d| d.as_str())
                .map(|d| d.trim())
                .filter(|d| !d.is_empty())
                .map(|d| {
                    if d.starts_with("-D") {
                        d.to_string()
                    } else {
                        format!("-D{}", d)
                    }
                })
                .collect();
            if !flags.is_empty() {
                args.push("--build-property".into());
                args.push(format!("compiler.cpp.extra_flags={}", flags.join(" ")).into());
            }
        }
    }

    args.push(sketch_dir.as_os_str().to_owned());

    send_progress("[compile-server] Spawning arduino-cli...\n", Some(0.1));

    // Spawn process.
    let mut cmd = std::process::Command::new(&config.arduino_cli_path);
    cmd.args(&args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", config.arduino_cli_path.display(), e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let send_cb = send_progress.clone();

    // Stream output.
    let mut output_log = String::new();
    if let Some(out) = stdout {
        let reader = BufReader::new(out);
        for line in reader.lines().flatten() {
            let prog = parse_progress_percentage(&line);
            send_cb(&format!("{}\n", line), prog);
            output_log.push_str(&line);
            output_log.push('\n');
        }
    }

    if let Some(err) = stderr {
        let reader = BufReader::new(err);
        for line in reader.lines().flatten() {
            send_cb(&format!("{}\n", line), None);
            output_log.push_str(&line);
            output_log.push('\n');
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Process wait failed: {}", e))?;

    if !status.success() {
        return Err(format!(
            "Compilation failed with exit code: {}\nLog tail:\n{}",
            status.code().unwrap_or(-1),
            tail_lines(&output_log, 15)
        ));
    }

    send_progress("[compile-server] Compilation succeeded! Collecting artifacts...\n", Some(0.95));

    // Prepare permanent artifact folder for this job.
    let artifacts_root = std::env::temp_dir().join("windify_artifacts");
    let job_artifact_dir = artifacts_root.join(job_id.to_string());
    fs::create_dir_all(&job_artifact_dir).map_err(|e| e.to_string())?;

    // Copy generated binaries.
    let manifest = collect_and_save_artifacts(&build_dir, &job_artifact_dir, &fqbn)?;

    // Write manifest JSON.
    let manifest_path = job_artifact_dir.join("manifest.json");
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(&manifest_path, manifest_bytes).map_err(|e| e.to_string())?;

    send_progress("[compile-server] Artifacts ready for download!\n", Some(1.0));

    let download_url = format!("/api/compile/{}/download", job_id);
    Ok((job_artifact_dir, download_url))
}

fn resolve_fqbn(config: &Option<Value>) -> String {
    if let Some(cfg) = config {
        if let Some(s) = cfg.get("fqbn").and_then(|v| v.as_str()) {
            return s.to_string();
        }
        if let Some(obj) = cfg.get("fqbn").and_then(|v| v.as_object()) {
            let key = if cfg!(target_os = "windows") {
                "win32"
            } else if cfg!(target_os = "macos") {
                "darwin"
            } else {
                "linux"
            };
            if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
    }
    String::new()
}

fn extract_windify_extra_files(code: &str) -> (String, BTreeMap<String, String>) {
    const START_PREFIX: &str = "// WINDIFY_EXTRA_SKETCH_FILE:";
    const END_MARKER: &str = "// END_WINDIFY_EXTRA_SKETCH_FILE";
    let mut extra = BTreeMap::new();
    if code.is_empty() {
        return (code.to_string(), extra);
    }
    let lines: Vec<&str> = code.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut main_lines: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(rest) = line.strip_prefix(START_PREFIX) {
            let file_name = rest.trim().to_string();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && lines[i] != END_MARKER {
                body.push(lines[i].to_string());
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            if !file_name.is_empty() {
                extra.insert(file_name, format!("{}\n", body.join("\n")));
            }
            continue;
        }
        main_lines.push(line.to_string());
        i += 1;
    }
    (main_lines.join("\n"), extra)
}

fn parse_progress_percentage(line: &str) -> Option<f64> {
    if let Some(idx) = line.rfind('%') {
        let prefix = &line[..idx];
        let num_str: String = prefix
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(val) = num_str.parse::<f64>() {
            return Some((val / 100.0).clamp(0.0, 1.0));
        }
    }
    None
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        text.to_string()
    } else {
        lines[lines.len() - n..].join("\n")
    }
}

fn collect_and_save_artifacts(
    build_dir: &Path,
    dest_dir: &Path,
    fqbn: &str,
) -> Result<FlashManifest, String> {
    let lower_fqbn = fqbn.to_lowercase();
    let is_esp32_s3 = lower_fqbn.contains("esp32s3");
    let is_esp32 = lower_fqbn.starts_with("esp32:");

    let mut parts = Vec::new();

    // Default ESP32 offsets.
    let bootloader_offset = if is_esp32_s3 { 0x0 } else { 0x1000 };
    let partitions_offset = 0x8000;
    let app_offset = 0x10000;

    let entries = fs::read_dir(build_dir).map_err(|e| e.to_string())?;
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        let fname = match p.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if fname.ends_with(".bootloader.bin") {
            let dest = dest_dir.join("bootloader.bin");
            fs::copy(&p, &dest).map_err(|e| e.to_string())?;
            let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            parts.push(FlashPart {
                name: "bootloader".into(),
                offset: bootloader_offset,
                offset_hex: format!("0x{:X}", bootloader_offset),
                filename: "bootloader.bin".into(),
                size_bytes: size,
            });
        } else if fname.ends_with(".partitions.bin") {
            let dest = dest_dir.join("partitions.bin");
            fs::copy(&p, &dest).map_err(|e| e.to_string())?;
            let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            parts.push(FlashPart {
                name: "partitions".into(),
                offset: partitions_offset,
                offset_hex: format!("0x{:X}", partitions_offset),
                filename: "partitions.bin".into(),
                size_bytes: size,
            });
        } else if fname.ends_with(".bin") && !fname.ends_with(".merged.bin") {
            let dest = dest_dir.join("app.bin");
            fs::copy(&p, &dest).map_err(|e| e.to_string())?;
            let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            parts.push(FlashPart {
                name: "app".into(),
                offset: app_offset,
                offset_hex: format!("0x{:X}", app_offset),
                filename: "app.bin".into(),
                size_bytes: size,
            });
        } else if fname.ends_with(".hex") {
            // For AVR boards.
            let dest = dest_dir.join("firmware.hex");
            fs::copy(&p, &dest).map_err(|e| e.to_string())?;
            let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            parts.push(FlashPart {
                name: "firmware".into(),
                offset: 0x0,
                offset_hex: "0x0".into(),
                filename: "firmware.hex".into(),
                size_bytes: size,
            });
        }
    }

    // Sort parts by offset so flashing happens in linear memory order.
    parts.sort_by_key(|p| p.offset);

    let chip = if is_esp32_s3 {
        "esp32s3".to_string()
    } else if is_esp32 {
        "esp32".to_string()
    } else {
        "avr".to_string()
    };

    Ok(FlashManifest {
        target: if is_esp32 { "esp32".into() } else { "avr".into() },
        fqbn: fqbn.to_string(),
        chip,
        flash_mode: "dio".into(),
        flash_freq: "80m".into(),
        flash_size: "16MB".into(),
        parts,
    })
}

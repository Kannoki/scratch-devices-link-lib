//! Persistent, atomic library snapshots received from the web VM.
//!
//! Web-managed libraries must not be written into the downloaded toolchain:
//! a toolchain install/update replaces that directory atomically and would
//! discard files received while setup is running. Instead, keep them under
//! user data and pass that search root to Arduino CLI with higher priority.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub libraries_updated: usize,
    pub files_written: usize,
    pub bytes_written: usize,
    pub warnings: Vec<String>,
}

fn validate_component(component: &str, label: &str) -> Result<(), String> {
    if component.is_empty() || component == "." || component == ".." {
        return Err(format!("{label} contains an empty or relative component"));
    }
    if component.len() > 180 {
        return Err(format!("{label} component is too long"));
    }
    if component.ends_with([' ', '.'])
        || component
            .chars()
            .any(|ch| ch.is_control() || r#"<>:"/\|?*"#.contains(ch))
    {
        return Err(format!("{label} contains characters unsafe on Windows"));
    }

    let upper = component.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    let reserved = matches!(stem, "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
        || stem
            .strip_prefix("LPT")
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n));
    if reserved {
        return Err(format!("{label} uses a reserved Windows name"));
    }
    Ok(())
}

fn validate_library_name(name: &str) -> Result<(), String> {
    validate_component(name, "library name")
}

fn safe_file_path(library_name: &str, raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() || raw.starts_with(['/', '\\']) {
        return Err(format!(
            "invalid file path in library {library_name}: {raw}"
        ));
    }
    let normalized = raw.replace('\\', "/");
    let mut components: Vec<&str> = normalized.split('/').collect();
    if components.first().copied() == Some(library_name) {
        components.remove(0);
    }
    if components.is_empty() {
        return Err(format!("file path only names library {library_name}"));
    }

    let mut relative = PathBuf::new();
    for component in components {
        validate_component(component, "library file path")?;
        relative.push(component);
    }
    Ok(relative)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("read synced libraries {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("read synced library entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("read type of {}: {error}", source_path.display()))?;
        if file_type.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "copy synced library file {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "unsupported symlink or special file in synced libraries: {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn replace_atomically(prepared: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let backup = parent.join(format!(".web-libraries-{}.backup", Uuid::new_v4().simple()));
    let had_previous = destination.exists();
    if had_previous {
        fs::rename(destination, &backup).map_err(|error| {
            format!(
                "move previous synced libraries {} aside: {error}",
                destination.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(prepared, destination) {
        if had_previous {
            let _ = fs::rename(&backup, destination);
        }
        return Err(format!(
            "activate synced libraries at {}: {error}",
            destination.display()
        ));
    }
    if had_previous {
        if let Err(error) = fs::remove_dir_all(&backup) {
            tracing::warn!(
                "[libraries] could not remove previous synced snapshot {}: {error}",
                backup.display()
            );
        }
    }
    Ok(())
}

fn acquire_sync_lock(root: &Path) -> Result<File, String> {
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "create library data directory {}: {error}",
            parent.display()
        )
    })?;
    let lock_path = parent.join(".web-libraries-sync.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("open library sync lock {}: {error}", lock_path.display()))?;
    file.lock()
        .map_err(|error| format!("lock library sync {}: {error}", lock_path.display()))?;
    Ok(file)
}

fn address_conflict_warnings(library_names: impl Iterator<Item = String>) -> Vec<String> {
    let normalized: Vec<String> = library_names
        .map(|name| {
            name.chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        })
        .collect();
    let has_tcs = normalized.iter().any(|name| name.contains("tcs34725"));
    let has_vl53 = normalized.iter().any(|name| name.contains("vl53l0x"));
    if has_tcs && has_vl53 {
        vec![
            "TCS34725 and VL53L0X both default to I2C address 0x29. Use separate I2C buses, an I2C multiplexer, or isolate/power-gate one device while assigning VL53L0X a different address."
                .to_string(),
        ]
    } else {
        Vec::new()
    }
}

/// Read installed library version from disk (from `library.properties` or `.version`).
fn read_installed_library_version(lib_dir: &Path) -> Option<String> {
    let version_file = lib_dir.join(".version");
    if let Ok(content) = fs::read_to_string(&version_file) {
        let v = content.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }

    let props_file = lib_dir.join("library.properties");
    if let Ok(content) = fs::read_to_string(&props_file) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("version=") {
                let v = rest.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }

    None
}

/// Compare two version strings. Returns true if installed version satisfies/matches expected version.
/// Supports dotted numeric comparison e.g. "2.0.6" >= "2.0.0" and exact string equality.
fn is_version_up_to_date(installed: &str, expected: &str) -> bool {
    if installed == expected {
        return true;
    }

    let parse_parts = |s: &str| -> Option<Vec<u64>> {
        s.split('.')
            .map(|p| p.trim().parse::<u64>().ok())
            .collect()
    };

    if let (Some(inst_parts), Some(exp_parts)) = (parse_parts(installed), parse_parts(expected)) {
        inst_parts >= exp_parts
    } else {
        installed == expected
    }
}

/// Check which of the requested libraries exist on disk under `root`,
/// verifying that installed versions are up-to-date.
///
/// Accepts `params` with:
/// - `names`: `["LibA", "LibB"]` (list of library names)
/// - `versions`: `{"LibA": "2.0.6", "LibB": "1.0.0"}` (optional expected versions)
/// - or `libraries`: `{"LibA": "2.0.6"}` / `{"LibA": { "version": "2.0.6" }}`
///
/// Returns:
/// `{ "present": [...], "missing": [...], "outdated": [...], "installedVersions": { ... } }`
pub fn check_libraries(root: &Path, params: &Value) -> Value {
    let mut names_set = std::collections::BTreeSet::new();
    let mut expected_versions = std::collections::BTreeMap::new();

    // 1. Extract from params.names (array of strings or objects {name, version})
    if let Some(arr) = params.get("names").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                names_set.insert(s.to_string());
            } else if let Some(obj) = item.as_object() {
                if let Some(n) = obj.get("name").and_then(|v| v.as_str()) {
                    names_set.insert(n.to_string());
                    if let Some(ver) = obj.get("version").and_then(|v| v.as_str()) {
                        expected_versions.insert(n.to_string(), ver.to_string());
                    }
                }
            }
        }
    }

    // 2. Extract from params.versions: { "LibA": "1.0.0" }
    if let Some(v_obj) = params.get("versions").and_then(|v| v.as_object()) {
        for (k, v) in v_obj {
            names_set.insert(k.clone());
            if let Some(ver_str) = v.as_str() {
                expected_versions.insert(k.clone(), ver_str.to_string());
            }
        }
    }

    // 3. Extract from params.libraries: { "LibA": "1.0.0" } or { "LibA": { "version": "1.0.0" } }
    if let Some(libs_obj) = params.get("libraries").and_then(|v| v.as_object()) {
        for (k, v) in libs_obj {
            names_set.insert(k.clone());
            if let Some(ver_str) = v.as_str() {
                expected_versions.insert(k.clone(), ver_str.to_string());
            } else if let Some(obj) = v.as_object() {
                if let Some(ver_str) = obj.get("version").and_then(|ver| ver.as_str()) {
                    expected_versions.insert(k.clone(), ver_str.to_string());
                }
            }
        }
    }

    // 4. Fallback if params is directly an array of names
    if let Some(arr) = params.as_array() {
        for item in arr {
            if let Some(s) = item.as_str() {
                names_set.insert(s.to_string());
            }
        }
    }

    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut outdated = Vec::new();
    let mut installed_versions = serde_json::Map::new();

    for name in names_set {
        if validate_library_name(&name).is_err() {
            missing.push(name);
            continue;
        }

        let lib_dir = root.join(&name);
        if !lib_dir.is_dir() {
            missing.push(name);
            continue;
        }

        let installed_ver = read_installed_library_version(&lib_dir);
        if let Some(ref ver) = installed_ver {
            installed_versions.insert(name.clone(), Value::String(ver.clone()));
        }

        if let Some(expected_ver) = expected_versions.get(&name) {
            match installed_ver {
                Some(ref actual) if is_version_up_to_date(actual, expected_ver) => {
                    present.push(name);
                }
                _ => {
                    // Outdated or unversioned: mark as missing so client re-syncs
                    outdated.push(name.clone());
                    missing.push(name);
                }
            }
        } else {
            // No version constraint specified: presence is sufficient
            present.push(name);
        }
    }

    serde_json::json!({
        "present": present,
        "missing": missing,
        "outdated": outdated,
        "installedVersions": installed_versions,
    })
}

/// Merge the supplied libraries into the persistent snapshot.
///
/// Each supplied library replaces its previous version completely (removing
/// stale files), while libraries omitted from this request are preserved.
pub fn sync_libraries(root: &Path, libraries: &Map<String, Value>) -> Result<SyncSummary, String> {
    if libraries.is_empty() {
        return Err("syncLibraries: 'libraries' must not be empty".to_string());
    }

    let _lock = acquire_sync_lock(root)?;
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    let stage = parent.join(format!(".web-libraries-{}.stage", Uuid::new_v4().simple()));
    let result = (|| {
        if root.exists() {
            copy_tree(root, &stage)?;
        } else {
            fs::create_dir_all(&stage).map_err(|error| {
                format!("create library sync stage {}: {error}", stage.display())
            })?;
        }

        let mut files_written = 0_usize;
        let mut bytes_written = 0_usize;
        for (library_name, files_value) in libraries {
            validate_library_name(library_name)
                .map_err(|error| format!("syncLibraries: {error}: {library_name}"))?;
            let files = files_value.as_object().ok_or_else(|| {
                format!("syncLibraries: library {library_name} must contain a file object")
            })?;
            if files.is_empty() {
                return Err(format!(
                    "syncLibraries: library {library_name} contains no files"
                ));
            }

            let library_stage = stage.join(library_name);
            if library_stage.exists() {
                fs::remove_dir_all(&library_stage).map_err(|error| {
                    format!(
                        "remove previous staged library {}: {error}",
                        library_stage.display()
                    )
                })?;
            }
            fs::create_dir_all(&library_stage).map_err(|error| {
                format!("create staged library {}: {error}", library_stage.display())
            })?;

            for (raw_path, content_value) in files {
                let content = content_value.as_str().ok_or_else(|| {
                    format!("syncLibraries: {library_name}/{raw_path} content must be a string")
                })?;
                files_written += 1;
                bytes_written = bytes_written.saturating_add(content.len());

                let relative = safe_file_path(library_name, raw_path)
                    .map_err(|error| format!("syncLibraries: {error}"))?;
                let target = library_stage.join(relative);
                if let Some(target_parent) = target.parent() {
                    fs::create_dir_all(target_parent).map_err(|error| {
                        format!(
                            "create library directory {}: {error}",
                            target_parent.display()
                        )
                    })?;
                }
                fs::write(&target, content).map_err(|error| {
                    format!("write synced library file {}: {error}", target.display())
                })?;
            }

            // Ensure library.properties exists and record detected version
            let props_path = library_stage.join("library.properties");
            let mut detected_version = None;
            if props_path.exists() {
                if let Ok(content) = fs::read_to_string(&props_path) {
                    for line in content.lines() {
                        if let Some(rest) = line.trim().strip_prefix("version=") {
                            let v = rest.trim();
                            if !v.is_empty() {
                                detected_version = Some(v.to_string());
                                break;
                            }
                        }
                    }
                }
            } else {
                let default_props = format!(
                    "name={}\nversion=1.0.0\nauthor=Windify\nmaintainer=Windify <support@windify.vn>\nsentence=Embedded by Windify Scratch Editor\nparagraph=Auto-bundled library\ncategory=Device Control\narchitectures=*\n",
                    library_name
                );
                let props_len = default_props.len();
                fs::write(&props_path, &default_props).map_err(|error| {
                    format!("write library.properties for {}: {error}", library_name)
                })?;
                files_written += 1;
                bytes_written = bytes_written.saturating_add(props_len);
                detected_version = Some("1.0.0".to_string());
            }

            // Persist fast-lookup .version marker file
            if let Some(ver) = detected_version {
                let version_file = library_stage.join(".version");
                let _ = fs::write(&version_file, ver);
            }
        }

        replace_atomically(&stage, root)?;
        Ok(SyncSummary {
            libraries_updated: libraries.len(),
            files_written,
            bytes_written,
            warnings: address_conflict_warnings(libraries.keys().cloned()),
        })
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{label}-{}", Uuid::new_v4().simple()))
    }

    #[test]
    fn atomically_merges_libraries_and_removes_stale_files() {
        let root = test_root("web-libraries");
        let first = json!({
            "Adafruit_TCS34725": {
                "Adafruit_TCS34725/src/Adafruit_TCS34725.h": "// color",
                "stale.h": "// stale"
            },
            "KeepMe": {
                "src/KeepMe.h": "// keep"
            }
        });
        sync_libraries(&root, first.as_object().unwrap()).unwrap();

        let second = json!({
            "Adafruit_TCS34725": {
                "src/Adafruit_TCS34725.h": "// updated"
            },
            "Adafruit_VL53L0X": {
                "src/Adafruit_VL53L0X.h": "// distance"
            }
        });
        let summary = sync_libraries(&root, second.as_object().unwrap()).unwrap();

        assert_eq!(summary.libraries_updated, 2);
        // 2 library header files + 2 auto-generated library.properties
        assert_eq!(summary.files_written, 4);
        assert_eq!(summary.warnings.len(), 1);
        assert_eq!(
            fs::read_to_string(root.join("Adafruit_TCS34725/src/Adafruit_TCS34725.h")).unwrap(),
            "// updated"
        );
        assert!(!root.join("Adafruit_TCS34725/stale.h").exists());
        assert!(root.join("KeepMe/src/KeepMe.h").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_library_path_traversal_without_touching_current_snapshot() {
        let root = test_root("web-libraries-traversal");
        let good = json!({"Safe": {"src/Safe.h": "// safe"}});
        sync_libraries(&root, good.as_object().unwrap()).unwrap();

        let malicious = json!({"Safe": {"../escaped.h": "// bad"}});
        assert!(sync_libraries(&root, malicious.as_object().unwrap()).is_err());
        assert!(root.join("Safe/src/Safe.h").exists());
        assert!(!root.parent().unwrap().join("escaped.h").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verifies_library_version_checks_and_outdated_detection() {
        let root = test_root("web-libraries-versions");
        let libs = json!({
            "LibOld": {
                "library.properties": "name=LibOld\nversion=1.0.0\nauthor=Test\n",
                "src/LibOld.h": "// old"
            },
            "LibNew": {
                "library.properties": "name=LibNew\nversion=2.0.6\nauthor=Test\n",
                "src/LibNew.h": "// new"
            }
        });
        sync_libraries(&root, libs.as_object().unwrap()).unwrap();

        // 1. Query with expected versions
        let params = json!({
            "names": ["LibOld", "LibNew", "LibMissing"],
            "versions": {
                "LibOld": "1.5.0",   // Expected 1.5.0, installed 1.0.0 -> outdated!
                "LibNew": "2.0.0"    // Expected 2.0.0, installed 2.0.6 -> up-to-date!
            }
        });

        let check = check_libraries(&root, &params);
        let present = check["present"].as_array().unwrap();
        let missing = check["missing"].as_array().unwrap();
        let outdated = check["outdated"].as_array().unwrap();
        let installed = check["installedVersions"].as_object().unwrap();

        assert!(present.iter().any(|v| v.as_str() == Some("LibNew")));
        assert!(!present.iter().any(|v| v.as_str() == Some("LibOld")));

        assert!(missing.iter().any(|v| v.as_str() == Some("LibMissing")));
        assert!(missing.iter().any(|v| v.as_str() == Some("LibOld"))); // Outdated is also in missing

        assert!(outdated.iter().any(|v| v.as_str() == Some("LibOld")));
        assert!(!outdated.iter().any(|v| v.as_str() == Some("LibNew")));

        assert_eq!(installed["LibOld"].as_str(), Some("1.0.0"));
        assert_eq!(installed["LibNew"].as_str(), Some("2.0.6"));

        fs::remove_dir_all(root).unwrap();
    }
}

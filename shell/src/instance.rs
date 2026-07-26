//! Process-wide single-instance guard.
//!
//! Starting a second tray process while the first one is installing tools can
//! interrupt extraction and leave a partial package behind. An OS file lock is
//! released automatically even if the process crashes or is terminated, so it
//! avoids both concurrent tray instances and stale PID/marker-file problems.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

pub struct InstanceGuard {
    _file: File,
}

pub enum AcquireOutcome {
    Acquired(InstanceGuard),
    AlreadyRunning,
}

fn default_lock_path() -> PathBuf {
    #[cfg(unix)]
    let file_name = format!("future-academy-link-{}.instance.lock", unsafe {
        libc::geteuid()
    });
    #[cfg(not(unix))]
    let file_name = "future-academy-link.instance.lock".to_string();

    std::env::temp_dir().join(file_name)
}

fn acquire_at(path: &Path) -> Result<AcquireOutcome, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create single-instance lock directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("open single-instance lock {}: {error}", path.display()))?;

    match file.try_lock() {
        Ok(()) => Ok(AcquireOutcome::Acquired(InstanceGuard { _file: file })),
        Err(TryLockError::WouldBlock) => Ok(AcquireOutcome::AlreadyRunning),
        Err(TryLockError::Error(error)) => Err(format!(
            "lock single-instance file {}: {error}",
            path.display()
        )),
    }
}

pub fn acquire() -> Result<AcquireOutcome, String> {
    acquire_at(&default_lock_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn allows_only_one_instance_and_recovers_after_drop() {
        let root =
            std::env::temp_dir().join(format!("windy-instance-test-{}", Uuid::new_v4().simple()));
        let lock_path = root.join("instance.lock");

        let first = match acquire_at(&lock_path).unwrap() {
            AcquireOutcome::Acquired(guard) => guard,
            AcquireOutcome::AlreadyRunning => panic!("first instance should acquire the lock"),
        };
        assert!(matches!(
            acquire_at(&lock_path).unwrap(),
            AcquireOutcome::AlreadyRunning
        ));

        drop(first);
        assert!(matches!(
            acquire_at(&lock_path).unwrap(),
            AcquireOutcome::Acquired(_)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}

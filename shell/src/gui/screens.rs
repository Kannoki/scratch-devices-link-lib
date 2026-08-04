//! Screen definitions for the Future Academy Link GUI.
//!
//! This module contains the Screen state enum and DeviceInfo struct.

/// Screen states for the application
#[derive(Clone, Debug)]
pub enum Screen {
    /// Downloading toolchain
    Downloading { progress: f32 },
    /// Extracting toolchain
    Extracting { progress: f32 },
    /// Starting/checking CLI
    Starting,
    /// Main device list
    Ready {
        devices: Vec<DeviceInfo>,
    },
}

/// Device information for display
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub port: String,
    pub pid: String,
    pub vid: String,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            name: "WINDIFY V2".to_string(),
            port: "COMM9".to_string(),
            pid: "1001".to_string(),
            vid: "303A".to_string(),
        }
    }
}

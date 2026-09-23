//! Serial port operations + helpers. Port of `src/lib/serial-device-list.js`
//! plus the serial-specific helpers from `src/session/serialport.js`
//! (`_comNum`, `_normalizeUsbId`, `_isEsp32S3OtgDevice`, `_isTransientSerialError`,
//! `_resolveReconnectPort` ranking) and `src/upload/*` port re-resolution.
//!
//! The `serialport` crate's blocking API is wrapped in a dedicated OS thread per
//! open connection; bytes are forwarded to an mpsc channel consumed by the
//! per-connection actor in `ws::serialport_session`.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use serde::Serialize;
use serialport::{SerialPort, SerialPortType};

use crate::usb_id;

// ── Platform-tuned timing constants (win32 vs other). Load-bearing — do not
//    round. Port of the constants at the top of serialport.js / arduino.js. ──

pub const PERIPHERAL_UNPLUG_CHECK_INTERVAL_MS: u64 = 100;
pub const PERIPHERAL_UNPLUG_CLOSED_STREAK: u32 = if cfg!(target_os = "windows") { 15 } else { 8 };
pub const POST_OPEN_UNPLUG_GRACE_MS: u64 = 2500;

pub const POST_FLASH_RECONNECT_INITIAL_DELAY_MS: u64 = if cfg!(target_os = "windows") {
    800
} else {
    400
};
pub const POST_FLASH_RECONNECT_ATTEMPTS: u32 = 16;
pub const POST_FLASH_RECONNECT_RETRY_DELAY_MS: u64 = if cfg!(target_os = "windows") {
    700
} else {
    500
};
pub const POST_FLASH_OPEN_UNPLUG_GRACE_MS: u64 = if cfg!(target_os = "windows") {
    12000
} else {
    8000
};
pub const TRANSIENT_RECONNECT_ATTEMPTS: u32 = 12;
pub const TRANSIENT_RECONNECT_DELAY_MS: u64 = if cfg!(target_os = "windows") {
    500
} else {
    400
};
pub const PORT_LIST_POLL_INTERVAL_MS: u64 = 250;
pub const PORT_LIST_RECONNECT_MAX_WAIT_MS: u64 = if cfg!(target_os = "windows") {
    18000
} else {
    12000
};

pub const ESP_RECONNECT_VENDOR_IDS: [&str; 3] = ["303A", "10C4", "1A86"];

/// Espressif native USB VID for ESP32-S3 OTG / Serial-JTAG (not UART bridge).
pub const ESP32S3_OTG_VENDOR_ID: &str = "303A";
/// Known ESP32-S3 native USB product IDs (OTG port).
pub const ESP32S3_OTG_PRODUCT_IDS: [&str; 2] = ["1001", "0002"];

pub const SCAN_DEVICES_DEFAULT_TIMEOUT_MS: u64 = 10000;
pub const SCAN_DEVICES_BUFFER_LIMIT: usize = 64 * 1024;

/// A serial port entry as returned by enumeration (mirrors node-serialport's
/// `SerialPort.list()` shape, fields we consume).
#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceInfo {
    pub path: String,
    #[serde(rename = "vendorId")]
    pub vendor_id: Option<String>,
    #[serde(rename = "productId")]
    pub product_id: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(rename = "serialNumber")]
    pub serial_number: Option<String>,
    #[serde(rename = "friendlyName")]
    pub friendly_name: Option<String>,
}

/// Status-panel device entry. Port of `serial-device-list.js` output shape.
#[derive(Debug, Clone, Serialize)]
pub struct StatusDevice {
    pub name: String,
    pub path: String,
    #[serde(rename = "vendorId")]
    pub vendor_id: String,
    #[serde(rename = "productId")]
    pub product_id: String,
}

/// Enumerate serial ports. Cross-platform via the `serialport` crate.
pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for p in ports {
        let mut info = DeviceInfo {
            path: p.port_name.clone(),
            ..Default::default()
        };
        if let SerialPortType::UsbPort(usb) = p.port_type {
            info.vendor_id = Some(format!("{:04X}", usb.vid));
            info.product_id = Some(format!("{:04X}", usb.pid));
            info.manufacturer = usb.manufacturer;
            info.serial_number = usb.serial_number;
            // serialport crate exposes `product` rather than friendlyName.
            info.friendly_name = usb.product;
        }
        out.push(info);
    }
    Ok(out)
}

/// `hasUsbIds` filter + name format. Port of `listSerialDevices`.
pub fn list_status_devices() -> Result<Vec<StatusDevice>, String> {
    let mut devices: Vec<StatusDevice> = list_devices()?
        .into_iter()
        .filter(|d| {
            !d.path.is_empty()
                && d.vendor_id
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
                && d.product_id
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
        })
        .map(|d| {
            let vid = d.vendor_id.clone().unwrap_or_default().to_uppercase();
            let pid = d.product_id.clone().unwrap_or_default().to_uppercase();
            StatusDevice {
                name: format_device_name(&d),
                path: d.path.clone(),
                vendor_id: vid,
                product_id: pid,
            }
        })
        .collect();
    // numeric-aware sort by path (approximates localeCompare numeric:true)
    devices.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    Ok(devices)
}

/// Port of `formatDeviceName` in serial-device-list.js.
pub fn format_device_name(device: &DeviceInfo) -> String {
    let vid = device.vendor_id.clone().unwrap_or_default().to_uppercase();
    let pid = device.product_id.clone().unwrap_or_default().to_uppercase();
    let pnpid = if !vid.is_empty() && !pid.is_empty() {
        format!("USB\\VID_{}&PID_{}", vid, pid)
    } else {
        String::new()
    };
    let mapped = if !pnpid.is_empty() {
        usb_id::lookup(&pnpid)
    } else {
        None
    };
    let friendly = device
        .friendly_name
        .clone()
        .or_else(|| device.manufacturer.clone())
        .or_else(|| device.serial_number.clone());
    let base_name = mapped
        .map(|s| s.to_string())
        .or(friendly)
        .unwrap_or_else(|| "Unknown device".to_string());
    format!("{} ({})", base_name, device.path)
}

/// Port of `_normalizeUsbId`: strip 0x prefix, uppercase.
pub fn normalize_usb_id(raw: &str) -> String {
    let s = raw.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    s.to_uppercase()
}

/// Port of `_comNum`: extract a comparable numeric id from a serial path.
pub fn com_num(serial_path: &str) -> i64 {
    if serial_path.is_empty() {
        return -1;
    }
    // Windows: COMn
    if let Some(idx) = serial_path.to_uppercase().find("COM") {
        let rest: String = serial_path[idx + 3..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !rest.is_empty() {
            if let Ok(n) = rest.parse::<i64>() {
                return n;
            }
        }
    }
    // Unix: trailing digits
    let trailing: String = serial_path
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if !trailing.is_empty() {
        if let Ok(n) = trailing.parse::<i64>() {
            return n;
        }
    }
    -1
}

/// Port of `_isEsp32S3OtgDevice`.
pub fn is_esp32s3_otg_device(device: &DeviceInfo) -> bool {
    let vid = normalize_usb_id(device.vendor_id.as_deref().unwrap_or(""));
    let pid = normalize_usb_id(device.product_id.as_deref().unwrap_or(""));
    if vid != ESP32S3_OTG_VENDOR_ID {
        return false;
    }
    ESP32S3_OTG_PRODUCT_IDS.contains(&pid.as_str())
}

/// Port of `_isTransientSerialError` regex (case-insensitive substring set).
pub fn is_transient_serial_error(msg: &str) -> bool {
    const NEEDLES: [&str; 16] = [
        "disconnected",
        "not open",
        "file_not_found",
        "operation aborted",
        "ebadf",
        "enoent",
        "access denied",
        "unknown error code 31",
        "resource temporarily unavailable",
        "eagain",
        "framing",
        "break",
        "overrun",
        "parity",
        // extra spellings kept for direct mapping with the original alternation
        "no such file",
        "could not open",
    ];
    let lower = msg.to_lowercase();
    NEEDLES.iter().take(14).any(|n| lower.contains(n))
        || NEEDLES[14..].iter().any(|n| lower.contains(n))
}

/// Open configuration for a serial port.
#[derive(Debug, Clone)]
pub struct OpenConfig {
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub rts: bool,
    pub dtr: bool,
}

/// An opened serial port handle wrapping the blocking `serialport` object.
pub struct OpenPort {
    inner: Box<dyn SerialPort>,
    /// The path this port was opened on (tracked for re-resolution diagnostics).
    #[allow(dead_code)]
    pub path: String,
}

impl OpenPort {
    /// Open + configure (rts/dtr) a serial port. Port of the `connect()` open path.
    pub fn open(path: &str, cfg: &OpenConfig) -> Result<OpenPort, String> {
        let data_bits = match cfg.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        };
        let stop_bits = match cfg.stop_bits {
            2 => serialport::StopBits::Two,
            _ => serialport::StopBits::One,
        };
        let mut port = serialport::new(path, cfg.baud_rate)
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| e.to_string())?;
        port.write_request_to_send(cfg.rts)
            .map_err(|e| e.to_string())?;
        port.write_data_terminal_ready(cfg.dtr)
            .map_err(|e| e.to_string())?;
        Ok(OpenPort {
            inner: port,
            path: path.to_string(),
        })
    }

    /// Non-blocking-ish read of available bytes; returns Ok(empty) on timeout.
    pub fn read_chunk(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        match self.inner.read(buf) {
            Ok(n) => Ok(n),
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            // Windows error 995 (ERROR_OPERATION_ABORTED) fires when the port
            // handle is closed while a ReadFile is still pending. Treat this as
            // a clean "port closed" rather than a mysterious OS error.
            Err(ref e) if is_operation_aborted(e) => Err("port closed".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn write_all(&mut self, data: &[u8]) -> Result<usize, String> {
        self.inner.write_all(data).map_err(|e| e.to_string())?;
        self.inner.flush().map_err(|e| e.to_string())?;
        Ok(data.len())
    }

    /// Live baud-rate change + re-apply rts/dtr. Port of `updateBaudrate`.
    pub fn update_baud(&mut self, baud: u32, rts: bool, dtr: bool) -> Result<(), String> {
        self.inner.set_baud_rate(baud).map_err(|e| e.to_string())?;
        self.inner
            .write_request_to_send(rts)
            .map_err(|e| e.to_string())?;
        self.inner
            .write_data_terminal_ready(dtr)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Resolve the best reconnect port. Port of `_resolveReconnectPort` ranking
/// (exact path → same VID(/PID) → any ESP VID, ranked by VID priority +
/// COM-number proximity). Polls until the deadline.
pub fn resolve_reconnect_port(
    preferred_path: &str,
    cached: Option<&DeviceInfo>,
) -> Result<DeviceInfo, String> {
    if preferred_path.is_empty() {
        return Err("Missing serial path for reconnect".to_string());
    }
    let preferred_vid = cached
        .and_then(|c| c.vendor_id.as_deref())
        .map(normalize_usb_id)
        .unwrap_or_default();
    let preferred_pid = cached
        .and_then(|c| c.product_id.as_deref())
        .map(normalize_usb_id)
        .unwrap_or_default();
    let deadline = Instant::now() + Duration::from_millis(PORT_LIST_RECONNECT_MAX_WAIT_MS);

    loop {
        let list = list_devices().unwrap_or_default();

        if let Some(exact) = list.iter().find(|d| d.path == preferred_path) {
            return Ok(exact.clone());
        }

        if !preferred_vid.is_empty() {
            let mut vid_matches: Vec<DeviceInfo> = list
                .iter()
                .filter(|d| {
                    let vid = normalize_usb_id(d.vendor_id.as_deref().unwrap_or(""));
                    let pid = normalize_usb_id(d.product_id.as_deref().unwrap_or(""));
                    if vid != preferred_vid || d.path.is_empty() {
                        return false;
                    }
                    preferred_pid.is_empty() || pid == preferred_pid
                })
                .cloned()
                .collect();
            if !vid_matches.is_empty() {
                let pref_com = com_num(preferred_path);
                vid_matches.sort_by(|a, b| {
                    if pref_com > 0 {
                        let da = (com_num(&a.path) - pref_com).abs();
                        let db = (com_num(&b.path) - pref_com).abs();
                        if da != db {
                            return da.cmp(&db);
                        }
                    }
                    com_num(&b.path).cmp(&com_num(&a.path))
                });
                return Ok(vid_matches.remove(0));
            }
        }

        let esp_matches: Vec<DeviceInfo> = list
            .iter()
            .filter(|d| {
                let vid = normalize_usb_id(d.vendor_id.as_deref().unwrap_or(""));
                ESP_RECONNECT_VENDOR_IDS.contains(&vid.as_str()) && !d.path.is_empty()
            })
            .cloned()
            .collect();
        if !esp_matches.is_empty() {
            if let Some(d) = pick_best_esp_reconnect_device(&esp_matches, preferred_path) {
                return Ok(d);
            }
        }

        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(PORT_LIST_POLL_INTERVAL_MS));
    }

    Err(format!("Serial port not listed yet: {}", preferred_path))
}

/// Port of `_pickBestEspReconnectDevice`.
pub fn pick_best_esp_reconnect_device(
    devices: &[DeviceInfo],
    preferred_path: &str,
) -> Option<DeviceInfo> {
    if devices.is_empty() {
        return None;
    }
    let pref_com = com_num(preferred_path);
    let rank = |device: &DeviceInfo| -> i64 {
        let mut score = 0i64;
        if is_esp32s3_otg_device(device) {
            score += 100;
        }
        let vid = normalize_usb_id(device.vendor_id.as_deref().unwrap_or(""));
        if vid == ESP32S3_OTG_VENDOR_ID {
            score += 50;
        }
        if pref_com > 0 {
            score -= (com_num(&device.path) - pref_com).abs();
        }
        score
    };
    let mut sorted = devices.to_vec();
    sorted.sort_by(|a, b| rank(b).cmp(&rank(a)));
    sorted.into_iter().next()
}

/// Natural (numeric-aware) string comparison for path sorting.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let na = com_num(a);
    let nb = com_num(b);
    if na >= 0 && nb >= 0 && na != nb {
        return na.cmp(&nb);
    }
    a.cmp(b)
}

/// On Windows, error code 995 (`ERROR_OPERATION_ABORTED`) fires when a pending
/// `ReadFile` is cancelled because the serial port handle was closed from
/// another thread. Error 1167 (`ERROR_DEVICE_NOT_CONNECTED`) or error 2
/// (`ERROR_FILE_NOT_FOUND`) fire when the USB device resets. Treat these
/// as benign "port closed" signals.
fn is_operation_aborted(err: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        matches!(err.raw_os_error(), Some(995) | Some(1167) | Some(2))
    }
    #[cfg(not(windows))]
    {
        let _ = err;
        false
    }
}

/// Port availability status for diagnostics and pre-flight checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortStatus {
    Available,
    Busy(String),
    NotFound,
    Error(String),
}

impl std::fmt::Display for PortStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortStatus::Available => write!(f, "Available"),
            PortStatus::Busy(reason) => write!(f, "Busy: {}", reason),
            PortStatus::NotFound => write!(f, "Port not found"),
            PortStatus::Error(err) => write!(f, "Error: {}", err),
        }
    }
}

/// Check if a serial port is currently openable and not locked by another process.
pub fn check_port_availability(path: &str) -> PortStatus {
    if path.is_empty() {
        return PortStatus::NotFound;
    }
    match serialport::new(path, 115200)
        .timeout(Duration::from_millis(50))
        .open()
    {
        Ok(port) => {
            drop(port);
            PortStatus::Available
        }
        Err(e) => {
            let msg = e.to_string();
            let lower = msg.to_lowercase();
            if lower.contains("access is denied")
                || lower.contains("permission denied")
                || lower.contains("device or resource busy")
            {
                PortStatus::Busy(format!("Port in use by another application (Access is denied)"))
            } else if lower.contains("not found")
                || lower.contains("cannot find")
                || lower.contains("no such file")
            {
                PortStatus::NotFound
            } else if lower.contains("995") || lower.contains("operation aborted") {
                PortStatus::Busy("I/O operation aborted (Windows Error 995)".to_string())
            } else {
                PortStatus::Error(msg)
            }
        }
    }
}

/// Wait asynchronously until the port handle is fully released by the OS driver.
pub async fn wait_for_port_release(path: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if check_port_availability(path) == PortStatus::Available {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Wait synchronously until the port handle is fully released by the OS driver.
#[allow(dead_code)]
pub fn wait_for_port_release_sync(path: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if check_port_availability(path) == PortStatus::Available {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_com_numbers_correctly() {
        assert_eq!(com_num("COM1"), 1);
        assert_eq!(com_num("COM9"), 9);
        assert_eq!(com_num("COM10"), 10);
        assert_eq!(com_num("COM24"), 24);
        assert_eq!(com_num("\\\\.\\COM15"), 15);
        assert_eq!(com_num("/dev/ttyUSB0"), 0);
        assert_eq!(com_num("/dev/ttyACM1"), 1);
        assert_eq!(com_num(""), -1);
    }

    #[test]
    fn check_port_availability_returns_not_found_on_nonexistent() {
        assert_eq!(
            check_port_availability(""),
            PortStatus::NotFound
        );
        assert_eq!(
            check_port_availability("COM9999"),
            PortStatus::NotFound
        );
    }

    #[test]
    fn detects_windows_error_codes_in_is_operation_aborted() {
        let err_995 = std::io::Error::from_raw_os_error(995);
        let err_1167 = std::io::Error::from_raw_os_error(1167);
        let err_2 = std::io::Error::from_raw_os_error(2);
        let err_other = std::io::Error::from_raw_os_error(1234);

        #[cfg(windows)]
        {
            assert!(is_operation_aborted(&err_995));
            assert!(is_operation_aborted(&err_1167));
            assert!(is_operation_aborted(&err_2));
            assert!(!is_operation_aborted(&err_other));
        }
        #[cfg(not(windows))]
        {
            let _ = (err_995, err_1167, err_2, err_other);
        }
    }
}



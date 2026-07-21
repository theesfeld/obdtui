//! Adapter endpoint descriptors (path + link kind).

use crate::bus::LinkKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One host-side endpoint that may host an ELM/STN adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterEndpoint {
    /// Device path (example: `/dev/ttyUSB0`, `/dev/rfcomm0`).
    pub path: String,
    pub kind: LinkKind,
    /// Short human description.
    pub description: String,
    /// Optional Bluetooth MAC if known (uppercase `AA:BB:…`).
    #[serde(default)]
    pub bt_mac: Option<String>,
}

impl AdapterEndpoint {
    pub fn new(path: impl Into<String>, kind: LinkKind, description: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind,
            description: description.into(),
            bt_mac: None,
        }
    }

    /// Classify a device path by name heuristics.
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let s = path.to_string_lossy().to_string();
        let kind = classify_path(&s);
        let description: String = match kind {
            LinkKind::UsbSerial => "USB serial OBD adapter".into(),
            LinkKind::BluetoothSpp => "Bluetooth SPP (RFCOMM)".into(),
            LinkKind::Other => "Serial port".into(),
        };
        Self::new(s, kind, description)
    }
}

/// Classify a serial device path as USB, Bluetooth RFCOMM, or other.
pub fn classify_path(path: &str) -> LinkKind {
    let n = path.to_ascii_lowercase();
    if n.contains("rfcomm")
        || n.contains("tty.bluetooth")
        || n.contains("bthmodem")
        || n.contains("/dev/cu.bluetooth")
        || n.contains("incoming-spp")
    {
        return LinkKind::BluetoothSpp;
    }
    if n.contains("ttyusb")
        || n.contains("ttyacm")
        || n.contains("cu.usb")
        || n.contains("usbserial")
        || n.contains("usbmodem")
    {
        return LinkKind::UsbSerial;
    }
    if n.contains("bluetooth") || n.contains("bt_") {
        return LinkKind::BluetoothSpp;
    }
    if n.contains("usb") {
        return LinkKind::UsbSerial;
    }
    LinkKind::Other
}

/// Normalize a Bluetooth MAC to `AA:BB:CC:DD:EE:FF` uppercase.
pub fn normalize_bt_mac(mac: &str) -> Option<String> {
    let hex: String = mac
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_uppercase();
    if hex.len() != 12 {
        return None;
    }
    let parts: Vec<&str> = (0..6).map(|i| &hex[i * 2..i * 2 + 2]).collect();
    Some(parts.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_usb() {
        assert_eq!(classify_path("/dev/ttyUSB0"), LinkKind::UsbSerial);
        assert_eq!(classify_path("/dev/ttyACM1"), LinkKind::UsbSerial);
    }

    #[test]
    fn classify_rfcomm() {
        assert_eq!(classify_path("/dev/rfcomm0"), LinkKind::BluetoothSpp);
        assert_eq!(classify_path("/dev/rfcomm3"), LinkKind::BluetoothSpp);
    }

    #[test]
    fn normalize_mac() {
        assert_eq!(
            normalize_bt_mac("aa-bb-cc-dd-ee-ff").as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert_eq!(
            normalize_bt_mac("AABBCCDDEEFF").as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert!(normalize_bt_mac("bad").is_none());
    }
}

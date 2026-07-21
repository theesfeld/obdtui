//! Bus tags for dual-bus capture (HS-CAN / MS-CAN) and host link kind.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Host link type for an OBD adapter (USB serial vs Bluetooth SPP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// USB CDC / USB-serial (FTDI, CH340, CP210x, etc.).
    UsbSerial,
    /// Bluetooth classic Serial Port Profile via RFCOMM.
    BluetoothSpp,
    /// Other or unclassified serial path.
    #[default]
    Other,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::UsbSerial => "usb",
            LinkKind::BluetoothSpp => "bluetooth",
            LinkKind::Other => "other",
        }
    }

    /// Typical baud rates to try for this link (first is preferred).
    pub fn default_bauds(self) -> &'static [u32] {
        match self {
            LinkKind::UsbSerial => &[38400, 115200, 9600],
            LinkKind::BluetoothSpp => &[38400, 115200, 9600],
            LinkKind::Other => &[38400, 115200, 9600],
        }
    }
}

impl fmt::Display for LinkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LinkKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "usb" | "usb_serial" | "wired" | "serial" => Ok(LinkKind::UsbSerial),
            "bluetooth" | "bt" | "spp" | "rfcomm" | "bluetooth_spp" => Ok(LinkKind::BluetoothSpp),
            "other" | "auto" => Ok(LinkKind::Other),
            other => Err(format!("unknown link kind: {other}")),
        }
    }
}

/// Preference when auto-selecting among discovered endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkPrefer {
    /// Prefer any working endpoint; USB first, then Bluetooth.
    #[default]
    Auto,
    /// Prefer USB serial endpoints.
    Usb,
    /// Prefer Bluetooth RFCOMM endpoints.
    Bluetooth,
}

impl FromStr for LinkPrefer {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(LinkPrefer::Auto),
            "usb" | "wired" => Ok(LinkPrefer::Usb),
            "bluetooth" | "bt" | "spp" => Ok(LinkPrefer::Bluetooth),
            other => Err(format!("unknown prefer: {other} (auto|usb|bluetooth)")),
        }
    }
}

impl fmt::Display for LinkPrefer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkPrefer::Auto => write!(f, "auto"),
            LinkPrefer::Usb => write!(f, "usb"),
            LinkPrefer::Bluetooth => write!(f, "bluetooth"),
        }
    }
}

/// Vehicle bus tag for capture and transport metadata.
///
/// Phase 0 always records a tag. Single-bus adapters use [`BusTag::Hs`] or
/// [`BusTag::Unknown`]. Dual-bus adapters may tag [`BusTag::Ms`] when MS-CAN
/// is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BusTag {
    /// High-speed CAN (typical powertrain OBD).
    Hs,
    /// Medium-speed CAN (Ford body modules; needs capable adapter).
    Ms,
    /// Bus is not known or not applicable.
    #[default]
    Unknown,
}

impl BusTag {
    /// Short label for TUI and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            BusTag::Hs => "hs",
            BusTag::Ms => "ms",
            BusTag::Unknown => "unknown",
        }
    }
}

impl fmt::Display for BusTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for BusTag {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "hs" | "hscan" | "hs-can" => Ok(BusTag::Hs),
            "ms" | "mscan" | "ms-can" => Ok(BusTag::Ms),
            "unknown" | "" => Ok(BusTag::Unknown),
            other => Err(format!("unknown bus tag: {other}")),
        }
    }
}

/// Adapter capability flags discovered at init.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdapterCapabilities {
    /// Adapter responds to ELM/STN AT commands.
    pub elm_compatible: bool,
    /// Adapter reports STN firmware (better multi-protocol support).
    pub stn: bool,
    /// Adapter can address MS-CAN (Ford dual-bus). Phase 0 may remain false.
    pub ms_can: bool,
    /// Host link: USB serial or Bluetooth SPP.
    #[serde(default)]
    pub link: LinkKind,
    /// Free-text adapter identity (ATI / STDI).
    pub identity: String,
    /// Active protocol string from ATDP if known.
    pub protocol: String,
}

impl AdapterCapabilities {
    /// Human summary for TUI status line.
    pub fn summary(&self) -> String {
        let kind = if self.stn {
            "STN"
        } else if self.elm_compatible {
            "ELM"
        } else {
            "unknown"
        };
        let ms = if self.ms_can {
            "MS-CAN: yes"
        } else {
            "MS-CAN: no"
        };
        format!("{kind} · {} · {ms} · {}", self.link, self.identity)
    }
}

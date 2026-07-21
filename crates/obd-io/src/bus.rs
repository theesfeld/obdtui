//! Bus tags for dual-bus capture (HS-CAN / MS-CAN).

use serde::{Deserialize, Serialize};
use std::fmt;

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
        format!("{kind} · {ms} · {}", self.identity)
    }
}

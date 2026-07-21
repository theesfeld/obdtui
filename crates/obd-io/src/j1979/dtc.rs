//! Diagnostic trouble code parse and commands.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// DTC readiness / bucket for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DtcStatus {
    Stored,
    Pending,
    Permanent,
}

/// One diagnostic trouble code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dtc {
    pub code: String,
    pub status: DtcStatus,
    pub raw: String,
}

/// Mode 03 / 07 / 0A request helpers.
pub fn dtc_command(mode: u8) -> String {
    format!("{mode:02X}")
}

/// Mode 04 clear DTCs — write-gated at session layer.
pub fn clear_dtc_command() -> &'static str {
    "04"
}

/// Parse Mode 03/07/0A style responses into DTC list.
pub fn parse_dtcs(response: &str, status: DtcStatus) -> Result<Vec<Dtc>> {
    if response.contains("NO DATA") || response.trim().is_empty() {
        return Ok(Vec::new());
    }

    let hex: String = response.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let bytes = crate::j1979::parse_hex_bytes(&hex)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    // Response service = mode + 0x40. Optional count byte follows on some ECUs.
    let mut i = 0;
    if bytes[0] >= 0x40 {
        i = 1;
        // If next looks like a small count and remaining length matches 2*count, skip it.
        if i < bytes.len() {
            let count = bytes[i] as usize;
            if count > 0 && count <= 16 && i + 1 + count * 2 == bytes.len() {
                i += 1;
            }
        }
    }

    let mut out = Vec::new();
    while i + 1 < bytes.len() {
        let a = bytes[i];
        let b = bytes[i + 1];
        i += 2;
        if a == 0 && b == 0 {
            continue;
        }
        let code = format_dtc(a, b);
        out.push(Dtc {
            code: code.clone(),
            status,
            raw: format!("{a:02X}{b:02X}"),
        });
    }
    Ok(out)
}

fn format_dtc(a: u8, b: u8) -> String {
    let system = match a >> 6 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        _ => 'U',
    };
    let n1 = (a >> 4) & 0x03;
    let n2 = a & 0x0F;
    format!("{system}{n1}{n2:X}{b:02X}")
}

/// Validate that clear was acknowledged (very soft).
pub fn parse_clear_ack(response: &str) -> Result<()> {
    let upper = response.to_ascii_uppercase();
    if upper.contains("NO DATA") || upper.contains("UNABLE") || upper.contains("ERROR") {
        return Err(Error::Protocol(format!("clear DTC failed: {response}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_p0301() {
        // P0301 = 03 01
        let dtcs = parse_dtcs("430301", DtcStatus::Stored).unwrap();
        assert_eq!(dtcs.len(), 1);
        assert_eq!(dtcs[0].code, "P0301");
    }

    #[test]
    fn empty_no_data() {
        let dtcs = parse_dtcs("NO DATA", DtcStatus::Stored).unwrap();
        assert!(dtcs.is_empty());
    }
}

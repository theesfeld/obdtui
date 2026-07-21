//! Mode 01 PID decode and Mode 09 helpers.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Definition of a standard OBD PID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidDef {
    pub mode: u8,
    pub pid: u8,
    pub name: String,
    pub unit: String,
    pub description: String,
}

/// One decoded live value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveValue {
    pub mode: u8,
    pub pid: u8,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub raw_hex: String,
}

/// Built-in PID table for the live dashboard.
pub fn standard_pid_defs() -> Vec<PidDef> {
    vec![
        pid(0x01, 0x0C, "engine_rpm", "rpm", "Engine RPM"),
        pid(0x01, 0x0D, "vehicle_speed", "km/h", "Vehicle speed"),
        pid(
            0x01,
            0x05,
            "coolant_temp",
            "C",
            "Engine coolant temperature",
        ),
        pid(0x01, 0x0F, "intake_temp", "C", "Intake air temperature"),
        pid(0x01, 0x11, "throttle", "%", "Throttle position"),
        pid(0x01, 0x04, "engine_load", "%", "Calculated engine load"),
        pid(0x01, 0x2F, "fuel_level", "%", "Fuel tank level"),
        pid(0x01, 0x46, "ambient_temp", "C", "Ambient air temperature"),
        pid(
            0x01,
            0x42,
            "control_module_voltage",
            "V",
            "Control module voltage",
        ),
    ]
}

fn pid(mode: u8, pid: u8, name: &str, unit: &str, description: &str) -> PidDef {
    PidDef {
        mode,
        pid,
        name: name.into(),
        unit: unit.into(),
        description: description.into(),
    }
}

/// Build Mode 01 request string (no spaces), e.g. `010C`.
pub fn mode01_command(pid: u8) -> String {
    format!("01{pid:02X}")
}

/// Build Mode 09 request string, e.g. `0902`.
pub fn mode09_command(pid: u8) -> String {
    format!("09{pid:02X}")
}

/// Parse whitespace-tolerant hex response into bytes.
pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.len() % 2 != 0 {
        return Err(Error::Decode(format!("odd hex length in response: {s:?}")));
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let h = std::str::from_utf8(&bytes[i..i + 2]).map_err(|e| Error::Decode(e.to_string()))?;
        let b = u8::from_str_radix(h, 16).map_err(|e| Error::Decode(e.to_string()))?;
        out.push(b);
        i += 2;
    }
    Ok(out)
}

/// Decode a Mode 01 response line into a live value when possible.
pub fn decode_pid(response: &str, expect_pid: Option<u8>) -> Result<LiveValue> {
    // Prefer last non-empty line; multi-line for some adapters.
    let line = response
        .lines()
        .map(str::trim)
        .rfind(|l| {
            !l.is_empty()
                && !l.starts_with("SEARCHING")
                && !l.contains("NO DATA")
                && !l.contains("UNABLE")
                && !l.contains("ERROR")
        })
        .ok_or_else(|| Error::Decode(format!("empty or error response: {response:?}")))?;

    let bytes = parse_hex_bytes(line)?;
    if bytes.len() < 3 {
        return Err(Error::Decode(format!("response too short: {line}")));
    }

    // Expected service response = mode + 0x40
    let mode = bytes[0].wrapping_sub(0x40);
    let pid = bytes[1];
    if let Some(ep) = expect_pid {
        if pid != ep {
            return Err(Error::Decode(format!(
                "PID mismatch: expected {ep:02X}, got {pid:02X}"
            )));
        }
    }

    let data = &bytes[2..];
    let (name, value, unit) = match (mode, pid) {
        (0x01, 0x0C) if data.len() >= 2 => {
            let rpm = ((data[0] as f64) * 256.0 + data[1] as f64) / 4.0;
            ("engine_rpm", rpm, "rpm")
        }
        (0x01, 0x0D) if !data.is_empty() => ("vehicle_speed", data[0] as f64, "km/h"),
        (0x01, 0x05) if !data.is_empty() => ("coolant_temp", data[0] as f64 - 40.0, "C"),
        (0x01, 0x0F) if !data.is_empty() => ("intake_temp", data[0] as f64 - 40.0, "C"),
        (0x01, 0x11) if !data.is_empty() => ("throttle", data[0] as f64 * 100.0 / 255.0, "%"),
        (0x01, 0x04) if !data.is_empty() => ("engine_load", data[0] as f64 * 100.0 / 255.0, "%"),
        (0x01, 0x2F) if !data.is_empty() => ("fuel_level", data[0] as f64 * 100.0 / 255.0, "%"),
        (0x01, 0x46) if !data.is_empty() => ("ambient_temp", data[0] as f64 - 40.0, "C"),
        (0x01, 0x42) if data.len() >= 2 => {
            let v = ((data[0] as f64) * 256.0 + data[1] as f64) / 1000.0;
            ("control_module_voltage", v, "V")
        }
        _ => {
            let raw = data
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join("");
            return Ok(LiveValue {
                mode,
                pid,
                name: format!("pid_{mode:02X}_{pid:02X}"),
                value: 0.0,
                unit: "raw".into(),
                raw_hex: raw,
            });
        }
    };

    Ok(LiveValue {
        mode,
        pid,
        name: name.into(),
        value,
        unit: unit.into(),
        raw_hex: line.chars().filter(|c| c.is_ascii_hexdigit()).collect(),
    })
}

/// Parse Mode 01 PID 00 (and optional 20/40/…) support bitmaps into PID numbers.
pub fn pid_supported_bitmap(response: &str, base: u8) -> Result<Vec<u8>> {
    let bytes = parse_hex_bytes(
        response
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or(response),
    )?;
    // 41 00 XX XX XX XX
    if bytes.len() < 6 {
        return Err(Error::Decode("support bitmap too short".into()));
    }
    let data = &bytes[2..6];
    let mut pids = Vec::new();
    for (byte_i, byte) in data.iter().enumerate() {
        for bit in 0..8 {
            if byte & (0x80 >> bit) != 0 {
                let pid = base + (byte_i as u8) * 8 + bit as u8 + 1;
                pids.push(pid);
            }
        }
    }
    Ok(pids)
}

/// Extract VIN ASCII from Mode 09 PID 02 multi-frame style responses.
pub fn parse_vin(response: &str) -> Result<String> {
    // ELM may return multi-line or single-line with frame markers:
    //   0: 49 02 01 31 46 54
    //   1: 45 57 31 45 50 39 4B
    // or jammed: 0140:4902013146541:4557314550394B2:46433733343939
    let stripped = strip_elm_frame_markers(response);
    let mut hex: String = stripped.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    // Drop ELM length prefix before service response (e.g. leading "014" before 4902).
    if let Some(pos) = hex.to_ascii_uppercase().find("4902") {
        hex = hex[pos..].to_string();
    }
    if hex.len() % 2 == 1 {
        hex = hex[1..].to_string();
    }
    if hex.len() < 6 {
        return Err(Error::Decode("VIN response too short".into()));
    }

    let bytes = parse_hex_bytes(&hex)?;
    // Payload after positive response 0x49 0x02, optional count 0x01.
    let payload = if let Some(i) = bytes.windows(2).position(|w| w == [0x49, 0x02]) {
        let rest = &bytes[i + 2..];
        if rest.first() == Some(&0x01) {
            &rest[1..]
        } else {
            rest
        }
    } else {
        bytes.as_slice()
    };

    let s: String = payload
        .iter()
        .filter(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        .map(|b| *b as char)
        .collect();
    if let Some(vin) = extract_vin17(&s) {
        return Ok(vin);
    }

    Err(Error::Decode(format!(
        "could not parse VIN from: {response:?}"
    )))
}

/// Remove ELM ISO-TP frame indices like `0:`, `1:`, `014:` from a response blob.
fn strip_elm_frame_markers(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        // Match 1–3 hex digits followed by ':'
        let mut j = i;
        while j < bytes.len() && j - i < 3 && bytes[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j > i && j < bytes.len() && bytes[j] == b':' {
            // Only treat as frame marker if preceded by start/whitespace/newline
            // or is near start of a frame group (always OK for ELM VIN dumps).
            let prev_ok =
                i == 0 || bytes[i - 1].is_ascii_whitespace() || !bytes[i - 1].is_ascii_hexdigit();
            // For jammed "0140:" the prev digit is hex — still a marker if total looks like count+index.
            // Prefer: any 1-digit index 0-9 before colon mid-stream is a frame index.
            let single_digit_index = j - i == 1;
            if prev_ok || single_digit_index {
                i = j + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn extract_vin17(s: &str) -> Option<String> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    if cleaned.len() >= 17 {
        // Prefer last 17 if prefix noise, else first 17.
        let vin = if cleaned.len() > 17 {
            cleaned[cleaned.len() - 17..].to_string()
        } else {
            cleaned
        };
        // VIN does not use I, O, Q typically — soft check only.
        if vin.len() == 17 {
            return Some(vin);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_decode() {
        // 0x1A F8 -> 1726 rpm
        let v = decode_pid("410C1AF8", Some(0x0C)).unwrap();
        assert_eq!(v.name, "engine_rpm");
        assert!((v.value - 1726.0).abs() < 0.1);
    }

    #[test]
    fn speed_decode() {
        let v = decode_pid("410D40", Some(0x0D)).unwrap();
        assert!((v.value - 64.0).abs() < 0.01);
    }

    #[test]
    fn coolant_decode() {
        let v = decode_pid("41056A", Some(0x05)).unwrap();
        assert!((v.value - 66.0).abs() < 0.01); // 0x6A - 40 = 66
    }

    #[test]
    fn vin_from_hex() {
        let vin = "1FTFW1E50MFA00000";
        let mut hex = String::from("490201");
        for b in vin.bytes() {
            hex.push_str(&format!("{b:02X}"));
        }
        let parsed = parse_vin(&hex).unwrap();
        assert_eq!(parsed, vin);
    }

    #[test]
    fn vin_from_elm_multiline() {
        let resp = "0: 49 02 01 31 46 54\n1: 45 57 31 45 50 39 4B\n2: 46 43 37 33 34 39 39";
        let parsed = parse_vin(resp).unwrap();
        assert_eq!(parsed, "1FTEW1EP9KFC73499");
    }

    #[test]
    fn vin_from_elm_jammed_line() {
        let resp = "0140:4902013146541:4557314550394B2:46433733343939";
        let parsed = parse_vin(resp).unwrap();
        assert_eq!(parsed, "1FTEW1EP9KFC73499");
    }
}

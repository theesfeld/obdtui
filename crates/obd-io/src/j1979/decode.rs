//! SAE J1979 Mode 01/02 PID decode formulas (common set).

use super::pid::{parse_hex_bytes, LiveValue};
use crate::error::{Error, Result};

/// Decode a Mode 01 or Mode 02 response for a known PID.
pub fn decode_mode_pid(
    response: &str,
    expect_mode: u8,
    expect_pid: Option<u8>,
) -> Result<LiveValue> {
    let line = response
        .lines()
        .map(str::trim)
        .rfind(|l| {
            !l.is_empty()
                && !l.starts_with("SEARCHING")
                && !l.contains("NO DATA")
                && !l.contains("UNABLE")
                && !l.contains("STOPPED")
                && !l.contains("ERROR")
        })
        .ok_or_else(|| Error::Decode(format!("empty or error response: {response:?}")))?;

    let bytes = parse_hex_bytes(line)?;
    if bytes.len() < 3 {
        return Err(Error::Decode(format!("response too short: {line}")));
    }

    let mode = bytes[0].wrapping_sub(0x40);
    let pid = bytes[1];
    if let Some(ep) = expect_pid {
        if pid != ep {
            return Err(Error::Decode(format!(
                "PID mismatch: expected {ep:02X}, got {pid:02X}"
            )));
        }
    }
    let _ = expect_mode;

    let data = &bytes[2..];
    match decode_payload(pid, data) {
        Some((name, value, unit)) => Ok(LiveValue {
            mode,
            pid,
            name,
            value,
            unit,
            raw_hex: line.chars().filter(|c| c.is_ascii_hexdigit()).collect(),
        }),
        None => {
            let raw = data
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join("");
            Ok(LiveValue {
                mode,
                pid,
                name: format!("pid_{mode:02X}_{pid:02X}"),
                value: 0.0,
                unit: "raw".into(),
                raw_hex: if raw.is_empty() {
                    line.chars().filter(|c| c.is_ascii_hexdigit()).collect()
                } else {
                    raw
                },
            })
        }
    }
}

fn decode_payload(pid: u8, data: &[u8]) -> Option<(String, f64, String)> {
    if data.is_empty() {
        return None;
    }
    let a = data[0] as f64;
    let b = data.get(1).copied().unwrap_or(0) as f64;

    let r = match pid {
        0x04 => ("engine_load", a * 100.0 / 255.0, "%"),
        0x05 => ("coolant_temp", a - 40.0, "C"),
        0x06 => ("stft_bank1", (a - 128.0) * 100.0 / 128.0, "%"),
        0x07 => ("ltft_bank1", (a - 128.0) * 100.0 / 128.0, "%"),
        0x08 => ("stft_bank2", (a - 128.0) * 100.0 / 128.0, "%"),
        0x09 => ("ltft_bank2", (a - 128.0) * 100.0 / 128.0, "%"),
        0x0A => ("fuel_pressure", a * 3.0, "kPa"),
        0x0B => ("map", a, "kPa"),
        0x0C if data.len() >= 2 => ("engine_rpm", (a * 256.0 + b) / 4.0, "rpm"),
        0x0D => ("vehicle_speed", a, "km/h"),
        0x0E => ("timing_advance", a / 2.0 - 64.0, "deg"),
        0x0F => ("intake_temp", a - 40.0, "C"),
        0x10 if data.len() >= 2 => ("maf", (a * 256.0 + b) / 100.0, "g/s"),
        0x11 => ("throttle", a * 100.0 / 255.0, "%"),
        0x1C => ("obd_standard", a, "code"),
        0x1F if data.len() >= 2 => ("run_time", a * 256.0 + b, "s"),
        0x21 if data.len() >= 2 => ("distance_mil", a * 256.0 + b, "km"),
        0x22 if data.len() >= 2 => ("fuel_rail_rel", (a * 256.0 + b) * 0.079, "kPa"),
        0x23 if data.len() >= 2 => ("fuel_rail_gauge", (a * 256.0 + b) * 10.0, "kPa"),
        0x2C => ("commanded_egr", a * 100.0 / 255.0, "%"),
        0x2D => ("egr_error", (a - 128.0) * 100.0 / 128.0, "%"),
        0x2E => ("commanded_evap", a * 100.0 / 255.0, "%"),
        0x2F => ("fuel_level", a * 100.0 / 255.0, "%"),
        0x30 => ("warmups_since_clear", a, "count"),
        0x31 if data.len() >= 2 => ("distance_clear", a * 256.0 + b, "km"),
        0x33 => ("baro", a, "kPa"),
        0x42 if data.len() >= 2 => ("control_module_voltage", (a * 256.0 + b) / 1000.0, "V"),
        0x43 if data.len() >= 2 => ("abs_load", (a * 256.0 + b) * 100.0 / 255.0, "%"),
        0x44 if data.len() >= 2 => ("cmd_equiv_ratio", (a * 256.0 + b) / 32768.0, "ratio"),
        0x45 => ("relative_throttle", a * 100.0 / 255.0, "%"),
        0x46 => ("ambient_temp", a - 40.0, "C"),
        0x47 => ("abs_throttle_b", a * 100.0 / 255.0, "%"),
        0x49 => ("accel_pedal_d", a * 100.0 / 255.0, "%"),
        0x4A => ("accel_pedal_e", a * 100.0 / 255.0, "%"),
        0x4C => ("commanded_throttle", a * 100.0 / 255.0, "%"),
        0x51 => ("fuel_type", a, "code"),
        0x52 => ("ethanol_fuel", a * 100.0 / 255.0, "%"),
        0x5A => ("rel_accel_pedal", a * 100.0 / 255.0, "%"),
        0x5B => ("hybrid_battery", a * 100.0 / 255.0, "%"),
        0x5C => ("oil_temp", a - 40.0, "C"),
        0x5E if data.len() >= 2 => ("fuel_rate", (a * 256.0 + b) / 20.0, "L/h"),
        0x61 => ("driver_demand_torque", a - 125.0, "%"),
        0x62 => ("actual_engine_torque", a - 125.0, "%"),
        0x63 if data.len() >= 2 => ("engine_ref_torque", a * 256.0 + b, "Nm"),
        _ => return None,
    };
    Some((r.0.into(), r.1, r.2.into()))
}

/// Catalog of PIDs we know how to decode (for dashboards / docs).
pub fn known_pid_catalog() -> Vec<(u8, &'static str, &'static str)> {
    vec![
        (0x04, "engine_load", "%"),
        (0x05, "coolant_temp", "C"),
        (0x06, "stft_bank1", "%"),
        (0x07, "ltft_bank1", "%"),
        (0x0A, "fuel_pressure", "kPa"),
        (0x0B, "map", "kPa"),
        (0x0C, "engine_rpm", "rpm"),
        (0x0D, "vehicle_speed", "km/h"),
        (0x0E, "timing_advance", "deg"),
        (0x0F, "intake_temp", "C"),
        (0x10, "maf", "g/s"),
        (0x11, "throttle", "%"),
        (0x1F, "run_time", "s"),
        (0x21, "distance_mil", "km"),
        (0x2F, "fuel_level", "%"),
        (0x31, "distance_clear", "km"),
        (0x33, "baro", "kPa"),
        (0x42, "control_module_voltage", "V"),
        (0x43, "abs_load", "%"),
        (0x45, "relative_throttle", "%"),
        (0x46, "ambient_temp", "C"),
        (0x47, "abs_throttle_b", "%"),
        (0x49, "accel_pedal_d", "%"),
        (0x4A, "accel_pedal_e", "%"),
        (0x4C, "commanded_throttle", "%"),
        (0x51, "fuel_type", "code"),
        (0x5C, "oil_temp", "C"),
        (0x5E, "fuel_rate", "L/h"),
        (0x61, "driver_demand_torque", "%"),
        (0x62, "actual_engine_torque", "%"),
        (0x63, "engine_ref_torque", "Nm"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm() {
        let v = decode_mode_pid("410C1AF8", 0x01, Some(0x0C)).unwrap();
        assert!((v.value - 1726.0).abs() < 0.1);
    }

    #[test]
    fn voltage() {
        // 0x36B0 = 14000 -> 14.0 V
        let v = decode_mode_pid("414236B0", 0x01, Some(0x42)).unwrap();
        assert!((v.value - 14.0).abs() < 0.1);
    }
}

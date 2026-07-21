//! SAE J1979 Mode 01 / 02 / 03 / 09 helpers.

mod decode;
mod dtc;
mod pid;

pub use decode::{decode_mode_pid, known_pid_catalog};
pub use dtc::{clear_dtc_command, dtc_command, parse_clear_ack, parse_dtcs, Dtc, DtcStatus};
pub use pid::{
    decode_pid, mode01_command, mode09_command, parse_hex_bytes, parse_vin, pid_supported_bitmap,
    priority_pids, standard_pid_defs, LiveValue, PidDef,
};

/// Mode 02 freeze-frame command for a PID (frame 0).
pub fn mode02_command(pid: u8) -> String {
    format!("02{pid:02X}00")
}

/// Mode 01 PID 01 monitor status request.
pub fn monitor_status_command() -> &'static str {
    "0101"
}

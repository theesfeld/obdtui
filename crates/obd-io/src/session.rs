//! High-level vehicle session: transport + capture + J1979 helpers.

use crate::bus::{AdapterCapabilities, BusTag};
use crate::capture::CaptureSession;
use crate::error::{Error, Result};
use crate::j1979::{
    clear_dtc_command, decode_mode_pid, decode_pid, dtc_command, mode01_command, mode02_command,
    mode09_command, parse_dtcs, parse_vin, pid_supported_bitmap, priority_pids, standard_pid_defs,
    Dtc, DtcStatus, LiveValue, PidDef,
};
use crate::profile::VehicleProfile;
use crate::transport::{Frame, Transport};
use std::path::Path;

/// High-rate PIDs vs bulk catalog for full capture.
const SUPPORT_BLOCKS: &[u8] = &[0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0];

/// Read-oriented session over any [`Transport`].
pub struct VehicleSession {
    transport: Box<dyn Transport>,
    pub profile: VehicleProfile,
    pub capture: Option<CaptureSession>,
    pub vin: Option<String>,
    pub supported_pids: Vec<u8>,
    pub calid: Option<String>,
    pub freeze_frame: Vec<LiveValue>,
    pub write_audit: Vec<String>,
    software: String,
    /// When false (default), clear-DTC is refused.
    pub allow_writes: bool,
    /// When capturing, poll every supported Mode 01 PID (F1 full capture).
    pub full_capture: bool,
}

impl VehicleSession {
    pub fn new(
        transport: Box<dyn Transport>,
        profile: VehicleProfile,
        software: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            profile,
            capture: None,
            vin: None,
            supported_pids: Vec::new(),
            calid: None,
            freeze_frame: Vec::new(),
            write_audit: Vec::new(),
            software: software.into(),
            allow_writes: false,
            full_capture: true,
        }
    }

    pub fn transport_name(&self) -> &str {
        self.transport.name()
    }

    pub fn capabilities(&self) -> &AdapterCapabilities {
        self.transport.capabilities()
    }

    pub fn active_bus(&self) -> BusTag {
        self.transport.active_bus()
    }

    pub fn init(&mut self) -> Result<()> {
        self.transport.init()?;
        Ok(())
    }

    pub fn start_capture(&mut self) {
        let mut cap = CaptureSession::new(&self.software);
        cap.meta.adapter_path = self.transport.name().to_string();
        cap.meta.capabilities = self.transport.capabilities().clone();
        cap.meta.profile_id = Some(self.profile.id.clone());
        cap.meta.notes = if self.full_capture {
            "full Mode 01 capture".into()
        } else {
            "dashboard capture".into()
        };
        if let Ok(yaml) = serde_yaml::to_string(&self.profile) {
            cap.vehicle_yaml = Some(yaml);
        }
        self.capture = Some(cap);
    }

    pub fn stop_capture(&mut self, dir: impl AsRef<Path>) -> Result<Option<()>> {
        if let Some(mut cap) = self.capture.take() {
            if let Some(vin) = &self.vin {
                cap.meta.vin = Some(vin.clone());
            }
            cap.save(dir)?;
            Ok(Some(()))
        } else {
            Ok(None)
        }
    }

    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }

    fn transact_recorded(&mut self, command: &str) -> Result<String> {
        let bus = self.transport.active_bus();
        if let Some(cap) = self.capture.as_mut() {
            cap.push_frame(Frame::tx(bus, command));
        }
        let resp = self.transport.transact(command)?;
        if let Some(cap) = self.capture.as_mut() {
            cap.push_frame(Frame::rx(bus, &resp));
        }
        Ok(resp)
    }

    pub fn read_vin(&mut self) -> Result<String> {
        let resp = self.transact_recorded(&mode09_command(0x02))?;
        let vin = parse_vin(&resp)?;
        self.vin = Some(vin.clone());
        Ok(vin)
    }

    /// Mode 09 CALID (calibration ID) when available.
    pub fn read_calid(&mut self) -> Result<String> {
        let resp = self.transact_recorded(&mode09_command(0x04))?;
        let s = extract_ascii_info(&resp);
        if s.is_empty() {
            return Err(Error::Decode("empty CALID".into()));
        }
        self.calid = Some(s.clone());
        Ok(s)
    }

    /// Enumerate all supported Mode 01 PID blocks (F1).
    pub fn probe_supported_pids(&mut self) -> Result<Vec<u8>> {
        let mut all = Vec::new();
        for &block in SUPPORT_BLOCKS {
            let resp = match self.transact_recorded(&mode01_command(block)) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let upper = resp.to_ascii_uppercase();
            if upper.contains("NO DATA") || upper.contains("STOPPED") {
                // No further blocks.
                if block == 0x00 {
                    break;
                }
                continue;
            }
            if let Ok(pids) = pid_supported_bitmap(&resp, block) {
                // Support bitmaps list PIDs in the next 0x20 range; PID = block itself
                // is the support PID and is not a live data signal.
                for p in pids {
                    if p != block && !all.contains(&p) {
                        all.push(p);
                    }
                }
            }
            // If bit 32 (next support page) not set, stop after this style check
            // Heuristic: if response too short, stop.
            if resp.chars().filter(|c| c.is_ascii_hexdigit()).count() < 12 && block > 0 {
                break;
            }
        }
        all.sort_unstable();
        self.supported_pids = all.clone();
        Ok(all)
    }

    pub fn read_pid(&mut self, pid: u8) -> Result<LiveValue> {
        let cmd = mode01_command(pid);
        let resp = self.transact_recorded(&cmd)?;
        let live = decode_pid(&resp, Some(pid))?;
        if let Some(cap) = self.capture.as_mut() {
            cap.push_live(&live, self.transport.active_bus());
        }
        Ok(live)
    }

    /// Mode 02 freeze frame for a PID (frame 0).
    pub fn read_freeze_pid(&mut self, pid: u8) -> Result<LiveValue> {
        let cmd = mode02_command(pid);
        let resp = self.transact_recorded(&cmd)?;
        let live = decode_mode_pid(&resp, 0x02, Some(pid))?;
        Ok(live)
    }

    /// Collect freeze-frame for priority PIDs (F3).
    pub fn read_freeze_frame(&mut self) -> Result<Vec<LiveValue>> {
        let mut out = Vec::new();
        for &pid in priority_pids() {
            if let Ok(v) = self.read_freeze_pid(pid) {
                out.push(v);
            }
        }
        self.freeze_frame = out.clone();
        Ok(out)
    }

    /// Monitor status Mode 01 PID 01 raw (F3).
    pub fn read_monitor_status_raw(&mut self) -> Result<String> {
        self.transact_recorded("0101")
    }

    /// Priority dashboard poll (fast set).
    pub fn poll_dashboard(&mut self) -> Result<Vec<LiveValue>> {
        let mut out = Vec::new();
        let pids: Vec<u8> = if self.supported_pids.is_empty() {
            priority_pids().to_vec()
        } else {
            priority_pids()
                .iter()
                .copied()
                .filter(|p| self.supported_pids.contains(p))
                .collect()
        };
        for pid in pids {
            if let Ok(v) = self.read_pid(pid) {
                out.push(v);
            }
        }
        if out.is_empty() {
            for pid in [0x0C, 0x0D, 0x05, 0x11] {
                if let Ok(v) = self.read_pid(pid) {
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    /// Full Mode 01 poll of every supported PID (F1). Used when capturing.
    pub fn poll_full(&mut self) -> Result<Vec<LiveValue>> {
        if self.supported_pids.is_empty() {
            let _ = self.probe_supported_pids();
        }
        let mut out = Vec::new();
        // Always include priority first for fresher critical signals.
        for &pid in priority_pids() {
            if self.supported_pids.is_empty() || self.supported_pids.contains(&pid) {
                if let Ok(v) = self.read_pid(pid) {
                    out.push(v);
                }
            }
        }
        for &pid in &self.supported_pids.clone() {
            if priority_pids().contains(&pid) {
                continue;
            }
            // Skip pure support PIDs
            if matches!(pid, 0x00 | 0x20 | 0x40 | 0x60 | 0x80 | 0xA0 | 0xC0) {
                continue;
            }
            if let Ok(v) = self.read_pid(pid) {
                out.push(v);
            }
        }
        Ok(out)
    }

    /// One capture tick: full or dashboard depending on `full_capture`.
    pub fn poll_for_ui_and_capture(&mut self) -> Result<Vec<LiveValue>> {
        if self.capture.is_some() && self.full_capture {
            self.poll_full()
        } else {
            self.poll_dashboard()
        }
    }

    pub fn read_dtcs(&mut self) -> Result<Vec<Dtc>> {
        let mut all = Vec::new();
        let stored = self.transact_recorded(&dtc_command(0x03))?;
        all.extend(parse_dtcs(&stored, DtcStatus::Stored)?);
        let pending = self.transact_recorded(&dtc_command(0x07))?;
        all.extend(parse_dtcs(&pending, DtcStatus::Pending).unwrap_or_default());
        // Permanent DTCs Mode 0A when available
        if let Ok(perm) = self.transact_recorded(&dtc_command(0x0A)) {
            all.extend(parse_dtcs(&perm, DtcStatus::Permanent).unwrap_or_default());
        }
        Ok(all)
    }

    /// Clear stored DTCs. Requires `allow_writes`. Audited (F5).
    pub fn clear_dtcs(&mut self) -> Result<()> {
        if !self.allow_writes {
            return Err(Error::WriteGated(
                "enable writes to clear DTCs (read-only default)".into(),
            ));
        }
        self.write_audit
            .push(format!("clear_dtcs bus={}", self.active_bus()));
        let resp = self.transact_recorded(clear_dtc_command())?;
        crate::j1979::parse_clear_ack(&resp)
    }

    pub fn select_bus(&mut self, bus: BusTag) -> Result<()> {
        self.transport.select_bus(bus)
    }

    /// Raw command for advanced use / TUI debug.
    pub fn raw(&mut self, command: &str) -> Result<String> {
        self.transact_recorded(command)
    }

    pub fn dashboard_pid_defs(&self) -> Vec<PidDef> {
        if self.profile.pids.is_empty() {
            standard_pid_defs()
        } else {
            self.profile.resolved_pids()
        }
    }

    /// Signal value by name from a live list.
    pub fn signal_map(values: &[LiveValue]) -> std::collections::HashMap<String, f64> {
        values.iter().map(|v| (v.name.clone(), v.value)).collect()
    }
}

fn extract_ascii_info(resp: &str) -> String {
    let hex: String = resp.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let Ok(bytes) = crate::j1979::parse_hex_bytes(&hex) else {
        return String::new();
    };
    bytes
        .iter()
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|b| *b as char)
        .collect::<String>()
        .trim()
        .to_string()
}

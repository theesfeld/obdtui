//! High-level vehicle session: transport + capture + J1979 helpers.

use crate::bus::{AdapterCapabilities, BusTag};
use crate::capture::CaptureSession;
use crate::error::{Error, Result};
use crate::j1979::{
    clear_dtc_command, decode_pid, dtc_command, mode01_command, mode09_command, parse_dtcs,
    parse_vin, standard_pid_defs, Dtc, DtcStatus, LiveValue, PidDef,
};
use crate::profile::VehicleProfile;
use crate::transport::{Frame, Transport};
use std::path::Path;

/// Read-oriented session over any [`Transport`].
pub struct VehicleSession {
    transport: Box<dyn Transport>,
    pub profile: VehicleProfile,
    pub capture: Option<CaptureSession>,
    pub vin: Option<String>,
    pub supported_pids: Vec<u8>,
    software: String,
    /// When false (default), clear-DTC is refused.
    pub allow_writes: bool,
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
            software: software.into(),
            allow_writes: false,
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

    pub fn probe_supported_pids(&mut self) -> Result<Vec<u8>> {
        let resp = self.transact_recorded(&mode01_command(0x00))?;
        let pids = crate::j1979::pid_supported_bitmap(&resp, 0x00).unwrap_or_default();
        self.supported_pids = pids.clone();
        Ok(pids)
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

    pub fn poll_dashboard(&mut self) -> Result<Vec<LiveValue>> {
        let defs: Vec<PidDef> = self.profile.resolved_pids();
        let mut out = Vec::new();
        for def in defs {
            if def.mode != 0x01 {
                continue;
            }
            // Skip if we know the ECU does not support it.
            if !self.supported_pids.is_empty() && !self.supported_pids.contains(&def.pid) {
                continue;
            }
            match self.read_pid(def.pid) {
                Ok(v) => out.push(v),
                Err(_) => continue,
            }
        }
        if out.is_empty() {
            // Fall back to a few common PIDs even if bitmap failed.
            for pid in [0x0C, 0x0D, 0x05, 0x11] {
                if let Ok(v) = self.read_pid(pid) {
                    out.push(v);
                }
            }
        }
        Ok(out)
    }

    pub fn read_dtcs(&mut self) -> Result<Vec<Dtc>> {
        let mut all = Vec::new();
        let stored = self.transact_recorded(&dtc_command(0x03))?;
        all.extend(parse_dtcs(&stored, DtcStatus::Stored)?);
        let pending = self.transact_recorded(&dtc_command(0x07))?;
        all.extend(parse_dtcs(&pending, DtcStatus::Pending).unwrap_or_default());
        Ok(all)
    }

    /// Clear stored DTCs. Requires `allow_writes`.
    pub fn clear_dtcs(&mut self) -> Result<()> {
        if !self.allow_writes {
            return Err(Error::WriteGated(
                "enable writes to clear DTCs (read-only default)".into(),
            ));
        }
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
}

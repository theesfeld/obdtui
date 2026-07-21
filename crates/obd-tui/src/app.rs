//! Application state for the TUI.

use chrono::Local;
use obd_io::{
    apply_stn_bus, ford_module_probes, probe_modules, BusTag, Dtc, LiveValue, VehicleSession,
};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Live,
    Gauges,
    Mfd,
    Dtc,
    Modules,
    Log,
    Help,
}

pub struct App {
    pub session: VehicleSession,
    pub tab: Tab,
    pub live: Vec<LiveValue>,
    pub dtcs: Vec<Dtc>,
    pub log: Vec<String>,
    pub live_poll: bool,
    pub capturing: bool,
    pub capture_root: PathBuf,
    pub status: String,
    pub last_error: Option<String>,
    pub gauge_index: usize,
    pub module_lines: Vec<String>,
    pub freeze_lines: Vec<String>,
}

impl App {
    pub fn new(session: VehicleSession, capture_root: PathBuf) -> Self {
        let status = format!(
            "{} · {}",
            session.transport_name(),
            session.capabilities().summary()
        );
        Self {
            session,
            tab: Tab::Live,
            live: Vec::new(),
            dtcs: Vec::new(),
            log: Vec::new(),
            live_poll: true,
            capturing: false,
            capture_root,
            status,
            last_error: None,
            gauge_index: 0,
            module_lines: Vec::new(),
            freeze_lines: Vec::new(),
        }
    }

    pub fn signal_map(&self) -> HashMap<String, f64> {
        VehicleSession::signal_map(&self.live)
    }

    pub fn push_log(&mut self, line: String) {
        let ts = Local::now().format("%H:%M:%S");
        self.log.push(format!("[{ts}] {line}"));
        if self.log.len() > 500 {
            let drain = self.log.len() - 400;
            self.log.drain(0..drain);
        }
    }

    pub fn next_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Live => Tab::Gauges,
            Tab::Gauges => Tab::Mfd,
            Tab::Mfd => Tab::Dtc,
            Tab::Dtc => Tab::Modules,
            Tab::Modules => Tab::Log,
            Tab::Log => Tab::Help,
            Tab::Help => Tab::Live,
        };
    }

    pub fn prev_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Live => Tab::Help,
            Tab::Gauges => Tab::Live,
            Tab::Mfd => Tab::Gauges,
            Tab::Dtc => Tab::Mfd,
            Tab::Modules => Tab::Dtc,
            Tab::Log => Tab::Modules,
            Tab::Help => Tab::Log,
        };
    }

    pub fn poll_live(&mut self) {
        match self.session.poll_for_ui_and_capture() {
            Ok(values) => {
                if !values.is_empty() {
                    self.live = values;
                    self.last_error = None;
                }
            }
            Err(e) => {
                let msg = format!("poll failed: {e}");
                self.last_error = Some(msg.clone());
                self.push_log(msg);
            }
        }
    }

    pub fn refresh_dtcs(&mut self) {
        match self.session.read_dtcs() {
            Ok(dtcs) => {
                self.dtcs = dtcs;
                self.push_log(format!("DTC count: {}", self.dtcs.len()));
                self.last_error = None;
            }
            Err(e) => {
                let msg = format!("DTC read failed: {e}");
                self.last_error = Some(msg.clone());
                self.push_log(msg);
            }
        }
        if let Ok(ff) = self.session.read_freeze_frame() {
            self.freeze_lines = ff
                .iter()
                .map(|v| format!("FF {} = {:.2} {}", v.name, v.value, v.unit))
                .collect();
            self.push_log(format!("Freeze frame signals: {}", self.freeze_lines.len()));
        }
    }

    pub fn toggle_capture(&mut self) {
        if self.capturing {
            let name = format!("session-{}", Local::now().format("%Y%m%d-%H%M%S"));
            let dir = self.capture_root.join(name);
            match self.session.stop_capture(&dir) {
                Ok(Some(())) => {
                    self.capturing = false;
                    self.push_log(format!(
                        "FULL capture saved: {} ({} signals mode)",
                        dir.display(),
                        if self.session.full_capture {
                            "all Mode 01 PIDs"
                        } else {
                            "dashboard"
                        }
                    ));
                }
                Ok(None) => {
                    self.capturing = false;
                    self.push_log("No active capture.".into());
                }
                Err(e) => {
                    self.push_log(format!("Capture save failed: {e}"));
                }
            }
        } else {
            // Ensure PID enumeration before full capture
            if self.session.full_capture {
                match self.session.probe_supported_pids() {
                    Ok(p) => self.push_log(format!("Full capture: {} supported PIDs", p.len())),
                    Err(e) => self.push_log(format!("PID probe warn: {e}")),
                }
            }
            self.session.start_capture();
            self.capturing = true;
            self.push_log(
                "FULL Mode 01 capture started (raw frames + all decoded signals).".into(),
            );
        }
    }

    pub fn try_clear_dtcs(&mut self) {
        match self.session.clear_dtcs() {
            Ok(()) => {
                self.push_log("DTCs cleared (write audit logged).".into());
                self.refresh_dtcs();
            }
            Err(e) => {
                self.push_log(format!("Clear DTC refused or failed: {e}"));
            }
        }
    }

    pub fn cycle_bus(&mut self) {
        let next = match self.session.active_bus() {
            BusTag::Hs => BusTag::Ms,
            BusTag::Ms => BusTag::Hs,
            BusTag::Unknown => BusTag::Hs,
        };
        match apply_stn_bus(&mut self.session, next) {
            Ok(()) => self.push_log(format!("Bus select: {next} (STN/HS-MS path)")),
            Err(e) => {
                // Fall back to tag-only
                match self.session.select_bus(next) {
                    Ok(()) => {
                        self.push_log(format!("Bus tag: {next} (protocol select failed: {e})"))
                    }
                    Err(e2) => self.push_log(format!("Bus select failed: {e2}")),
                }
            }
        }
    }

    pub fn probe_modules(&mut self) {
        let probes = ford_module_probes();
        match probe_modules(&mut self.session, &probes) {
            Ok(results) => {
                self.module_lines = results
                    .iter()
                    .map(|r| {
                        if r.ok {
                            format!(
                                "OK  {} [{}] {}",
                                r.id,
                                r.bus,
                                r.response.chars().take(40).collect::<String>()
                            )
                        } else {
                            format!(
                                "FAIL {} [{}] {}",
                                r.id,
                                r.bus,
                                r.error.clone().unwrap_or_default()
                            )
                        }
                    })
                    .collect();
                self.push_log(format!("Module probes: {}", results.len()));
            }
            Err(e) => self.push_log(format!("Module probe failed: {e}")),
        }
    }

    pub fn next_gauge(&mut self) {
        self.gauge_index = self.gauge_index.wrapping_add(1);
    }
}

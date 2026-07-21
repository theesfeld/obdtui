//! Application state for the TUI.

use chrono::Local;
use obd_io::{BusTag, Dtc, LiveValue, VehicleSession};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Live,
    Dtc,
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
        }
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
            Tab::Live => Tab::Dtc,
            Tab::Dtc => Tab::Log,
            Tab::Log => Tab::Help,
            Tab::Help => Tab::Live,
        };
    }

    pub fn prev_tab(&mut self) {
        self.tab = match self.tab {
            Tab::Live => Tab::Help,
            Tab::Dtc => Tab::Live,
            Tab::Log => Tab::Dtc,
            Tab::Help => Tab::Log,
        };
    }

    pub fn poll_live(&mut self) {
        match self.session.poll_dashboard() {
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
    }

    pub fn toggle_capture(&mut self) {
        if self.capturing {
            let name = format!("session-{}", Local::now().format("%Y%m%d-%H%M%S"));
            let dir = self.capture_root.join(name);
            match self.session.stop_capture(&dir) {
                Ok(Some(())) => {
                    self.capturing = false;
                    self.push_log(format!("Capture saved: {}", dir.display()));
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
            self.session.start_capture();
            self.capturing = true;
            self.push_log("Capture started.".into());
        }
    }

    pub fn try_clear_dtcs(&mut self) {
        match self.session.clear_dtcs() {
            Ok(()) => {
                self.push_log("DTCs cleared.".into());
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
            BusTag::Ms => BusTag::Unknown,
            BusTag::Unknown => BusTag::Hs,
        };
        match self.session.select_bus(next) {
            Ok(()) => self.push_log(format!("Active bus: {next}")),
            Err(e) => self.push_log(format!("Bus select failed: {e}")),
        }
    }
}

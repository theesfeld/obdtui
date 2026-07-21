//! Replay transport from a capture session (offline / CI).

use super::{FrameDir, Transport};
use crate::bus::{AdapterCapabilities, BusTag};
use crate::capture::CaptureSession;
use crate::error::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Replays captured TX/RX pairs without hardware.
///
/// Responses are keyed by normalized command (uppercase, no spaces). Each
/// command keeps a FIFO of responses so repeated PID polls work.
pub struct ReplayTransport {
    name: String,
    caps: AdapterCapabilities,
    active_bus: BusTag,
    by_cmd: HashMap<String, VecDeque<String>>,
    /// Fallback FIFO when command was not in capture.
    order: VecDeque<String>,
}

impl ReplayTransport {
    pub fn from_session(session: &CaptureSession, name: impl Into<String>) -> Self {
        let mut by_cmd: HashMap<String, VecDeque<String>> = HashMap::new();
        let mut order = VecDeque::new();
        let frames = &session.frames;
        let mut i = 0;
        while i < frames.len() {
            if frames[i].dir == FrameDir::Tx {
                let cmd = normalize_cmd(&frames[i].data);
                let mut j = i + 1;
                let mut resp = String::new();
                while j < frames.len() && frames[j].dir != FrameDir::Tx {
                    if frames[j].dir == FrameDir::Rx {
                        if !resp.is_empty() {
                            resp.push('\n');
                        }
                        resp.push_str(&frames[j].data);
                    }
                    j += 1;
                }
                by_cmd.entry(cmd).or_default().push_back(resp.clone());
                order.push_back(resp);
                i = j;
            } else {
                i += 1;
            }
        }

        Self {
            name: name.into(),
            caps: session.meta.capabilities.clone(),
            active_bus: BusTag::Hs,
            by_cmd,
            order,
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let session = CaptureSession::load(path.as_ref())?;
        let name = path.as_ref().display().to_string();
        Ok(Self::from_session(&session, name))
    }

    pub fn remaining_responses(&self) -> usize {
        self.by_cmd.values().map(|q| q.len()).sum()
    }
}

fn normalize_cmd(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase()
}

impl Transport for ReplayTransport {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        &self.caps
    }

    fn active_bus(&self) -> BusTag {
        self.active_bus
    }

    fn select_bus(&mut self, bus: BusTag) -> Result<()> {
        self.active_bus = bus;
        Ok(())
    }

    fn init(&mut self) -> Result<()> {
        if self.caps.identity.is_empty() {
            self.caps.identity = "replay".into();
            self.caps.elm_compatible = true;
        }
        Ok(())
    }

    fn transact(&mut self, command: &str) -> Result<String> {
        let key = normalize_cmd(command);
        if let Some(q) = self.by_cmd.get_mut(&key) {
            if let Some(resp) = q.pop_front() {
                // Keep last response for further polls of same PID.
                if q.is_empty() {
                    q.push_back(resp.clone());
                }
                return Ok(resp);
            }
        }
        // Soft success for AT init commands not in capture.
        if key.starts_with("AT") || key.starts_with("ST") {
            return Ok("OK".into());
        }
        self.order
            .pop_front()
            .ok_or_else(|| Error::Protocol(format!("replay has no response for {command}")))
    }
}

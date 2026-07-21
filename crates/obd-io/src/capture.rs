//! Versioned capture session package for reuse in other projects.

use crate::bus::{AdapterCapabilities, BusTag};
use crate::error::{Error, Result};
use crate::j1979::LiveValue;
use crate::transport::Frame;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Capture format version (bump when on-disk schema breaks).
pub const CAPTURE_FORMAT_VERSION: u32 = 1;

/// Session metadata (`meta.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub format_version: u32,
    pub software: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub vin: Option<String>,
    #[serde(default)]
    pub adapter_path: String,
    #[serde(default)]
    pub capabilities: AdapterCapabilities,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub notes: String,
}

impl SessionMeta {
    pub fn new(software: impl Into<String>) -> Self {
        Self {
            format_version: CAPTURE_FORMAT_VERSION,
            software: software.into(),
            started_at: Utc::now(),
            ended_at: None,
            vin: None,
            adapter_path: String::new(),
            capabilities: AdapterCapabilities::default(),
            profile_id: None,
            notes: String::new(),
        }
    }
}

/// One decoded signal sample for timeseries export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSample {
    pub ts: DateTime<Utc>,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub mode: u8,
    pub pid: u8,
    pub bus: BusTag,
}

/// Full capture session in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSession {
    pub meta: SessionMeta,
    pub frames: Vec<Frame>,
    pub signals: Vec<SignalSample>,
    /// Optional vehicle snapshot YAML text.
    #[serde(default)]
    pub vehicle_yaml: Option<String>,
}

impl CaptureSession {
    pub fn new(software: impl Into<String>) -> Self {
        Self {
            meta: SessionMeta::new(software),
            frames: Vec::new(),
            signals: Vec::new(),
            vehicle_yaml: None,
        }
    }

    pub fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub fn push_signal(&mut self, sample: SignalSample) {
        self.signals.push(sample);
    }

    pub fn push_live(&mut self, live: &LiveValue, bus: BusTag) {
        self.signals.push(SignalSample {
            ts: Utc::now(),
            name: live.name.clone(),
            value: live.value,
            unit: live.unit.clone(),
            mode: live.mode,
            pid: live.pid,
            bus,
        });
    }

    /// Write session package directory:
    /// `meta.toml`, `frames.ndjson`, `signals.csv`, optional `vehicle.yaml`.
    pub fn save(&mut self, dir: impl AsRef<Path>) -> Result<()> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        self.meta.ended_at = Some(Utc::now());

        let meta_toml = toml::to_string_pretty(&self.meta)?;
        fs::write(dir.join("meta.toml"), meta_toml)?;

        let mut frames_file = File::create(dir.join("frames.ndjson"))?;
        for frame in &self.frames {
            serde_json::to_writer(&mut frames_file, frame)?;
            frames_file.write_all(b"\n")?;
        }

        let mut csv = File::create(dir.join("signals.csv"))?;
        writeln!(csv, "ts,name,value,unit,mode,pid,bus")?;
        for s in &self.signals {
            writeln!(
                csv,
                "{},{},{},{},{},{},{}",
                s.ts.to_rfc3339(),
                escape_csv(&s.name),
                s.value,
                escape_csv(&s.unit),
                s.mode,
                s.pid,
                s.bus
            )?;
        }

        if let Some(yaml) = &self.vehicle_yaml {
            fs::write(dir.join("vehicle.yaml"), yaml)?;
        }

        // Machine-readable full dump for other projects.
        let full = serde_json::to_vec_pretty(self)?;
        fs::write(dir.join("session.json"), full)?;

        Ok(())
    }

    pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let session_json = dir.join("session.json");
        if session_json.exists() {
            let text = fs::read_to_string(&session_json)?;
            let session: CaptureSession = serde_json::from_str(&text)?;
            return Ok(session);
        }

        // Reconstruct from parts.
        let meta_text = fs::read_to_string(dir.join("meta.toml"))
            .map_err(|e| Error::Capture(format!("meta.toml: {e}")))?;
        let meta: SessionMeta = toml::from_str(&meta_text)?;

        let mut frames = Vec::new();
        let frames_path = dir.join("frames.ndjson");
        if frames_path.exists() {
            let file = File::open(frames_path)?;
            for line in BufReader::new(file).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                frames.push(serde_json::from_str(&line)?);
            }
        }

        let mut signals = Vec::new();
        let signals_path = dir.join("signals.csv");
        if signals_path.exists() {
            let file = File::open(signals_path)?;
            let mut lines = BufReader::new(file).lines();
            let _header = lines.next();
            for line in lines {
                let line = line?;
                if let Some(sample) = parse_signal_csv_line(&line) {
                    signals.push(sample);
                }
            }
        }

        let vehicle_yaml = {
            let p = dir.join("vehicle.yaml");
            if p.exists() {
                Some(fs::read_to_string(p)?)
            } else {
                None
            }
        };

        Ok(Self {
            meta,
            frames,
            signals,
            vehicle_yaml,
        })
    }
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn parse_signal_csv_line(line: &str) -> Option<SignalSample> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 7 {
        return None;
    }
    Some(SignalSample {
        ts: DateTime::parse_from_rfc3339(parts[0])
            .ok()?
            .with_timezone(&Utc),
        name: parts[1].to_string(),
        value: parts[2].parse().ok()?,
        unit: parts[3].to_string(),
        mode: parts[4].parse().ok()?,
        pid: parts[5].parse().ok()?,
        bus: parts[6].parse().unwrap_or(BusTag::Unknown),
    })
}

/// Helper to build a frame pair for fixtures.
pub fn frame_pair(bus: BusTag, tx: &str, rx: &str) -> (Frame, Frame) {
    (Frame::tx(bus, tx), Frame::rx(bus, rx))
}

/// Default capture output directory under cwd.
pub fn default_capture_root() -> PathBuf {
    PathBuf::from("captures")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::FrameDir;

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = CaptureSession::new("obd-io-test");
        session.meta.vin = Some("1FTFW1E50MFA00000".into());
        session.push_frame(Frame::tx(BusTag::Hs, "010C"));
        session.push_frame(Frame::rx(BusTag::Hs, "410C1AF8"));
        session.push_live(
            &crate::j1979::decode_pid("410C1AF8", Some(0x0C)).unwrap(),
            BusTag::Hs,
        );
        session.save(dir.path()).unwrap();

        let loaded = CaptureSession::load(dir.path()).unwrap();
        assert_eq!(loaded.meta.vin.as_deref(), Some("1FTFW1E50MFA00000"));
        assert_eq!(loaded.frames.len(), 2);
        assert_eq!(loaded.frames[0].dir, FrameDir::Tx);
        assert_eq!(loaded.signals.len(), 1);
        assert!((loaded.signals[0].value - 1726.0).abs() < 0.1);
    }
}

//! Vehicle profile files (YAML) — multi-vehicle, data-driven.

use crate::error::{Error, Result};
use crate::j1979::{standard_pid_defs, PidDef};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// On-disk vehicle profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub makes: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub dual_bus: bool,
    #[serde(default)]
    pub pids: Vec<ProfilePid>,
    #[serde(default)]
    pub modules: Vec<ModuleStub>,
}

/// PID entry in a profile (extends or overrides standard).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilePid {
    pub mode: u8,
    pub pid: u8,
    pub name: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub description: String,
    /// Optional bus preference for dual-bus vehicles.
    #[serde(default)]
    pub bus: Option<String>,
}

/// Placeholder for future module maps (Ford ECU list, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleStub {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bus: Option<String>,
    #[serde(default)]
    pub notes: String,
}

impl VehicleProfile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(Error::Io)?;
        let profile: VehicleProfile = serde_yaml::from_str(&text).map_err(|e| Error::Profile {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        Ok(profile)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_yaml::to_string(self)?;
        fs::write(path, text)?;
        Ok(())
    }

    /// Merge profile PIDs with built-in standards (profile wins on same mode/pid).
    pub fn resolved_pids(&self) -> Vec<PidDef> {
        let mut map: Vec<PidDef> = standard_pid_defs();
        for p in &self.pids {
            if let Some(existing) = map.iter_mut().find(|x| x.mode == p.mode && x.pid == p.pid) {
                existing.name = p.name.clone();
                if !p.unit.is_empty() {
                    existing.unit = p.unit.clone();
                }
                if !p.description.is_empty() {
                    existing.description = p.description.clone();
                }
            } else {
                map.push(PidDef {
                    mode: p.mode,
                    pid: p.pid,
                    name: p.name.clone(),
                    unit: p.unit.clone(),
                    description: p.description.clone(),
                });
            }
        }
        map
    }
}

/// Load all `*.yaml` profiles from a directory.
pub fn load_profiles_dir(dir: impl AsRef<Path>) -> Result<Vec<VehicleProfile>> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml")
        {
            out.push(VehicleProfile::load(&path)?);
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Default search paths relative to cwd and optional install share.
pub fn default_profile_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("profiles")];
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        // When tests run from crate dir, also look at workspace profiles.
        dirs.push(PathBuf::from(manifest).join("../../profiles"));
    }
    dirs
}

/// Built-in generic profile when no files exist.
pub fn generic_profile() -> VehicleProfile {
    VehicleProfile {
        id: "generic_j1979".into(),
        name: "Generic J1979".into(),
        makes: vec!["*".into()],
        notes: "Standard OBD-II Mode 01/03/09 PIDs.".into(),
        dual_bus: false,
        pids: standard_pid_defs()
            .into_iter()
            .map(|p| ProfilePid {
                mode: p.mode,
                pid: p.pid,
                name: p.name,
                unit: p.unit,
                description: p.description,
                bus: None,
            })
            .collect(),
        modules: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_has_rpm() {
        let p = generic_profile();
        assert!(p.resolved_pids().iter().any(|x| x.pid == 0x0C));
    }
}

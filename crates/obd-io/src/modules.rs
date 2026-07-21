//! Ford / generic module read scaffold (safe read-only probes).

use crate::bus::BusTag;
use crate::error::{Error, Result};
use crate::session::VehicleSession;
use serde::{Deserialize, Serialize};

/// Known module probe target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleProbe {
    pub id: String,
    pub name: String,
    pub bus: BusTag,
    /// Optional raw ELM/STN setup commands before read.
    pub setup: Vec<String>,
    /// OBD/UDS-style request (hex) for a simple identity probe.
    pub probe_cmd: String,
    pub notes: String,
}

/// Default Ford module probe list (read-only; expands from captures over time).
pub fn ford_module_probes() -> Vec<ModuleProbe> {
    vec![
        ModuleProbe {
            id: "pcm".into(),
            name: "Powertrain Control Module".into(),
            bus: BusTag::Hs,
            setup: vec![],
            probe_cmd: "0902".into(), // VIN via standard OBD on powertrain
            notes: "Standard OBD on HS-CAN.".into(),
        },
        ModuleProbe {
            id: "obd_std".into(),
            name: "OBD functional".into(),
            bus: BusTag::Hs,
            setup: vec![],
            probe_cmd: "0100".into(),
            notes: "Supported PID bitmap.".into(),
        },
    ]
}

/// Result of one module probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleProbeResult {
    pub id: String,
    pub bus: String,
    pub ok: bool,
    pub response: String,
    pub error: Option<String>,
}

/// Run read-only probes on the session (does not program modules).
pub fn probe_modules(
    session: &mut VehicleSession,
    probes: &[ModuleProbe],
) -> Result<Vec<ModuleProbeResult>> {
    let mut out = Vec::new();
    for p in probes {
        if let Err(e) = session.select_bus(p.bus) {
            out.push(ModuleProbeResult {
                id: p.id.clone(),
                bus: p.bus.to_string(),
                ok: false,
                response: String::new(),
                error: Some(e.to_string()),
            });
            continue;
        }
        for cmd in &p.setup {
            let _ = session.raw(cmd);
        }
        match session.raw(&p.probe_cmd) {
            Ok(resp) => out.push(ModuleProbeResult {
                id: p.id.clone(),
                bus: p.bus.to_string(),
                ok: !resp.is_empty()
                    && !resp.to_ascii_uppercase().contains("UNABLE")
                    && !resp.to_ascii_uppercase().contains("ERROR"),
                response: resp,
                error: None,
            }),
            Err(e) => out.push(ModuleProbeResult {
                id: p.id.clone(),
                bus: p.bus.to_string(),
                ok: false,
                response: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
    // Return to HS bus for normal OBD.
    let _ = session.select_bus(BusTag::Hs);
    Ok(out)
}

/// STN/OBDLink protocol select helpers for dual-bus.
pub fn stn_select_hs() -> &'static [&'static str] {
    // ISO 15765-4 CAN 11/500 (HS-CAN)
    &["ATSP6"]
}

pub fn stn_select_ms() -> &'static [&'static str] {
    // STN protocol 53: ISO 15765-4 CAN 11 bit @ 125 kbaud (Ford MS-CAN class)
    // Also try STP 53 form used by STN firmwares.
    &["STP53", "ATSPB C350"]
}

/// Apply bus selection via raw AT/ST commands when transport select is limited.
pub fn apply_stn_bus(session: &mut VehicleSession, bus: BusTag) -> Result<()> {
    let cmds = match bus {
        BusTag::Hs => stn_select_hs(),
        BusTag::Ms => stn_select_ms(),
        BusTag::Unknown => return Ok(()),
    };
    let mut last_err = None;
    for c in cmds {
        match session.raw(c) {
            Ok(_) => {
                session.select_bus(bus)?;
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| Error::Protocol("could not select bus on this adapter".into())))
}

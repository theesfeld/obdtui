//! Sample capture builders and re-exports for offline OBD work.

use obd_io::bus::{AdapterCapabilities, BusTag, LinkKind};
use obd_io::capture::CaptureSession;
use obd_io::transport::{Frame, ReplayTransport};
use obd_io::Result;
use std::path::Path;

/// Build a minimal demo capture (Ford-shaped VIN, live PIDs, one DTC).
pub fn sample_session() -> CaptureSession {
    let mut session = CaptureSession::new("obd-sim");
    session.meta.vin = Some("1FTFW1E50MFA00000".into());
    session.meta.adapter_path = "replay://sample".into();
    session.meta.capabilities = AdapterCapabilities {
        elm_compatible: true,
        stn: false,
        ms_can: false,
        link: LinkKind::Other,
        identity: "ELM327 SAMPLE".into(),
        protocol: "ISO 15765-4 (CAN 11/500)".into(),
    };
    session.meta.profile_id = Some("generic_j1979".into());
    session.meta.notes = "Synthetic fixture for CI and offline TUI.".into();

    let pairs = [
        ("ATI", "ELM327 SAMPLE"),
        ("ATDP", "ISO 15765-4 (CAN 11/500)"),
        ("0902", "4902013146544657314535304D46413030303030"),
        ("0100", "4100BE3EA813"),
        ("010C", "410C1AF8"),
        ("010D", "410D00"),
        ("0105", "41056A"),
        ("0111", "411180"),
        ("0104", "410440"),
        ("03", "430301"),
        ("07", "470000"),
        // Extra cycles for dashboard polling
        ("010C", "410C1B00"),
        ("010D", "410D20"),
        ("0105", "41056B"),
        ("0111", "411190"),
        ("0104", "410450"),
        ("010C", "410C1C00"),
        ("010D", "410D30"),
        ("0105", "41056C"),
        ("0111", "4111A0"),
        ("0104", "410460"),
    ];

    for (tx, rx) in pairs {
        session.push_frame(Frame::tx(BusTag::Hs, tx));
        session.push_frame(Frame::rx(BusTag::Hs, rx));
    }

    if let Ok(live) = obd_io::j1979::decode_pid("410C1AF8", Some(0x0C)) {
        session.push_live(&live, BusTag::Hs);
    }

    session.vehicle_yaml = Some(
        r#"id: generic_j1979
name: Generic J1979
makes: ["*"]
notes: Sample fixture profile.
dual_bus: false
pids: []
modules: []
"#
        .into(),
    );

    session
}

/// Write sample session to `dir` and return a [`ReplayTransport`].
pub fn write_sample_and_replay(dir: impl AsRef<Path>) -> Result<ReplayTransport> {
    let mut session = sample_session();
    session.save(dir.as_ref())?;
    Ok(ReplayTransport::from_session(
        &session,
        dir.as_ref().display().to_string(),
    ))
}

/// Open a replay transport from an existing capture directory.
pub fn replay_from(path: impl AsRef<Path>) -> Result<ReplayTransport> {
    ReplayTransport::from_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obd_io::{generic_profile, VehicleSession};

    #[test]
    fn sample_replay_reads_rpm() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = sample_session();
        session.save(dir.path()).unwrap();
        let transport = ReplayTransport::from_path(dir.path()).unwrap();
        let mut vs = VehicleSession::new(Box::new(transport), generic_profile(), "obd-sim-test");
        vs.init().unwrap();
        let _ = vs.raw("ATI");
        let _ = vs.raw("ATDP");
        let vin = vs.read_vin().unwrap();
        assert!(vin.starts_with('1') || vin.len() == 17);
        let _ = vs.probe_supported_pids();
        let rpm = vs.read_pid(0x0C).unwrap();
        assert_eq!(rpm.name, "engine_rpm");
        assert!(rpm.value > 0.0);
    }
}

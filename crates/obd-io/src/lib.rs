//! `obd-io` — OBD-II transport, J1979 decode, profiles, and capture format.
//!
//! Default mode is **read-only**. Clear-DTC and other writes are gated.

pub mod bus;
pub mod capture;
pub mod error;
pub mod j1979;
pub mod modules;
pub mod profile;
pub mod session;
pub mod transport;

pub use bus::{AdapterCapabilities, BusTag, LinkKind, LinkPrefer};
pub use capture::{CaptureSession, SessionMeta, SignalSample, CAPTURE_FORMAT_VERSION};
pub use error::{Error, Result};
pub use j1979::{
    decode_mode_pid, known_pid_catalog, mode02_command, priority_pids, Dtc, DtcStatus, LiveValue,
    PidDef,
};
pub use modules::{
    apply_stn_bus, ford_module_probes, probe_modules, ModuleProbe, ModuleProbeResult,
};
pub use profile::{generic_profile, load_profiles_dir, VehicleProfile};
pub use session::VehicleSession;
pub use transport::{
    classify_path, connect, discover_adapters, discover_serial_ports, ensure_rfcomm,
    format_endpoint_list, normalize_bt_mac, release_rfcomm, AdapterEndpoint, BtSppTransport,
    ConnectOptions, ConnectedAdapter, ElmConfig, ElmTransport, Frame, FrameDir, ReplayTransport,
    Transport,
};

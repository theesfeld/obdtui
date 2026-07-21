//! `obd-io` — OBD-II transport, J1979 decode, profiles, and capture format.
//!
//! Default mode is **read-only**. Clear-DTC and other writes are gated.

pub mod bus;
pub mod capture;
pub mod error;
pub mod j1979;
pub mod profile;
pub mod session;
pub mod transport;

pub use bus::{AdapterCapabilities, BusTag};
pub use capture::{CaptureSession, SessionMeta, SignalSample, CAPTURE_FORMAT_VERSION};
pub use error::{Error, Result};
pub use j1979::{Dtc, DtcStatus, LiveValue, PidDef};
pub use profile::{generic_profile, load_profiles_dir, VehicleProfile};
pub use session::VehicleSession;
pub use transport::{
    discover_serial_ports, ElmConfig, ElmTransport, Frame, FrameDir, ReplayTransport, Transport,
};

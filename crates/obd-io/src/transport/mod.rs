//! Transport abstraction for live adapters and replay.

mod connect;
mod elm;
mod link;
mod replay;

pub use connect::{
    connect, discover_adapters, discover_serial_ports, ensure_rfcomm, format_endpoint_list,
    release_rfcomm, ConnectOptions, ConnectedAdapter,
};
pub use elm::{ElmConfig, ElmTransport};
pub use link::{classify_path, normalize_bt_mac, AdapterEndpoint};
pub use replay::ReplayTransport;

use crate::bus::{AdapterCapabilities, BusTag};
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Direction of a frame on the wire or in a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameDir {
    Tx,
    Rx,
}

/// One request or response unit (AT line or OBD payload text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub ts: DateTime<Utc>,
    pub dir: FrameDir,
    pub bus: BusTag,
    /// UTF-8 payload as exchanged with the adapter (no trailing `>` prompt).
    pub data: String,
}

impl Frame {
    pub fn tx(bus: BusTag, data: impl Into<String>) -> Self {
        Self {
            ts: Utc::now(),
            dir: FrameDir::Tx,
            bus,
            data: data.into(),
        }
    }

    pub fn rx(bus: BusTag, data: impl Into<String>) -> Self {
        Self {
            ts: Utc::now(),
            dir: FrameDir::Rx,
            bus,
            data: data.into(),
        }
    }
}

/// Bidirectional adapter or replay source.
pub trait Transport: Send {
    /// Open path or source description for status UI.
    fn name(&self) -> &str;

    /// Capabilities after successful init (empty before init).
    fn capabilities(&self) -> &AdapterCapabilities;

    /// Active bus for subsequent commands.
    fn active_bus(&self) -> BusTag;

    /// Select bus when the adapter supports dual-bus. Default rejects MS-CAN.
    fn select_bus(&mut self, bus: BusTag) -> Result<()> {
        if bus == BusTag::Ms && !self.capabilities().ms_can {
            return Err(crate::error::Error::Protocol(
                "adapter does not support MS-CAN".into(),
            ));
        }
        let _ = bus;
        Ok(())
    }

    /// Initialize the adapter (ATZ sequence or no-op for replay).
    fn init(&mut self) -> Result<()>;

    /// Send a command and wait for a full response (without `>`).
    fn transact(&mut self, command: &str) -> Result<String>;
}

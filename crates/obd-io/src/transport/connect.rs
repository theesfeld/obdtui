//! Discover and open USB or Bluetooth SPP ELM adapters.

use super::bt_spp::BtSppTransport;
use super::elm::{ElmConfig, ElmTransport};
use super::link::{normalize_bt_mac, AdapterEndpoint};
use super::Transport;
use crate::bus::{BusTag, LinkKind, LinkPrefer};
use crate::error::{Error, Result};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Options for opening a live ELM/STN adapter over USB or Bluetooth SPP.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Explicit device path. When set, discovery is skipped (except RFCOMM bind).
    pub path: Option<String>,
    /// Fixed baud. When `None`, try link-typical bauds.
    pub baud: Option<u32>,
    /// Auto-select preference among discovered endpoints.
    pub prefer: LinkPrefer,
    /// Bluetooth MAC: bind RFCOMM if path is not already an open RFCOMM node.
    pub bt_mac: Option<String>,
    /// RFCOMM device index (`/dev/rfcomm{N}`).
    pub rfcomm_index: u8,
    /// RFCOMM channel (usually 1 for ELM SPP).
    pub rfcomm_channel: u8,
    /// Per-transaction serial timeout.
    pub timeout: Duration,
    /// Default vehicle bus tag for captures.
    pub default_bus: BusTag,
    /// When true, do not run ELM init after open (tests only).
    pub skip_init: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            path: None,
            baud: None,
            prefer: LinkPrefer::Auto,
            bt_mac: None,
            rfcomm_index: 0,
            rfcomm_channel: 1,
            timeout: Duration::from_millis(2_500),
            default_bus: BusTag::Hs,
            skip_init: false,
        }
    }
}

/// Result of a successful connect.
pub struct ConnectedAdapter {
    pub transport: Box<dyn Transport>,
    pub endpoint: AdapterEndpoint,
    /// Serial baud used for USB/`/dev/rfcomm` paths. `0` for native RFCOMM sockets.
    pub baud: u32,
}

/// Discover USB-serial and Bluetooth RFCOMM endpoints.
pub fn discover_adapters() -> Result<Vec<AdapterEndpoint>> {
    let mut out: Vec<AdapterEndpoint> = Vec::new();

    let ports = serialport::available_ports().map_err(Error::Serial)?;
    for p in ports {
        let ep = AdapterEndpoint::from_path(&p.port_name);
        if is_candidate(&ep) {
            push_unique(&mut out, ep);
        }
    }

    // RFCOMM nodes may not show in serialport enumeration.
    for n in 0..8 {
        let path = format!("/dev/rfcomm{n}");
        if Path::new(&path).exists() {
            let mut ep = AdapterEndpoint::from_path(&path);
            ep.kind = LinkKind::BluetoothSpp;
            ep.description = format!("Bluetooth SPP RFCOMM {n}");
            push_unique(&mut out, ep);
        }
    }

    // Hints from paired BlueZ devices (does not open RFCOMM by itself).
    for hint in bluetooth_paired_hints() {
        // Only add if we have no RFCOMM yet for that MAC, as a synthetic row.
        if out.iter().any(|e| e.bt_mac.as_ref() == Some(&hint.mac)) {
            continue;
        }
        out.push(AdapterEndpoint {
            path: format!("bt://{}", hint.mac),
            kind: LinkKind::BluetoothSpp,
            description: format!("paired Bluetooth (not bound): {}", hint.name),
            bt_mac: Some(hint.mac),
        });
    }

    sort_endpoints(&mut out, LinkPrefer::Auto);
    Ok(out)
}

/// List device paths only (compat with older API).
pub fn discover_serial_ports() -> Result<Vec<String>> {
    Ok(discover_adapters()?
        .into_iter()
        .filter(|e| !e.path.starts_with("bt://"))
        .map(|e| e.path)
        .collect())
}

/// Connect to an adapter: wired USB and/or Bluetooth SPP.
///
/// Order:
/// 1. Bluetooth MAC / `bt://` → native RFCOMM socket (no root)
/// 2. Prefer paired Bluetooth hints when `--prefer bluetooth` or auto and no USB
/// 3. USB serial and existing `/dev/rfcomm*` with multi-baud try
pub fn connect(opts: ConnectOptions) -> Result<ConnectedAdapter> {
    let mut errors: Vec<String> = Vec::new();

    // 1) Explicit Bluetooth MAC → native SPP socket (preferred; no sudo).
    if let Some(mac) = opts.bt_mac.clone() {
        match try_bt_spp(&mac, &opts) {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                let msg = format!("bt-spp {mac}: {e}");
                warn!("{msg}");
                errors.push(msg);
            }
        }
    }

    // 2) Path is bt://MAC
    if let Some(path) = opts.path.clone() {
        if let Some(mac) = path.strip_prefix("bt://").and_then(normalize_bt_mac) {
            match try_bt_spp(&mac, &opts) {
                Ok(conn) => return Ok(conn),
                Err(e) => errors.push(format!("bt-spp {mac}: {e}")),
            }
        }
    }

    // 3) Prefer: try paired OBD Bluetooth hints via native SPP when bluetooth preferred
    //    or when auto and no explicit path.
    if opts.path.is_none()
        && opts.bt_mac.is_none()
        && matches!(opts.prefer, LinkPrefer::Auto | LinkPrefer::Bluetooth)
    {
        for hint in bluetooth_paired_hints() {
            match try_bt_spp(&hint.mac, &opts) {
                Ok(conn) => return Ok(conn),
                Err(e) => {
                    debug!(mac = %hint.mac, error = %e, "paired BT SPP try failed");
                    errors.push(format!("bt-spp {}: {e}", hint.mac));
                }
            }
        }
    }

    // 4) Serial paths (USB + /dev/rfcomm)
    let candidates = resolve_candidates(&opts)?;
    for ep in candidates {
        if ep.path.starts_with("bt://") {
            if let Some(mac) = ep
                .bt_mac
                .as_ref()
                .cloned()
                .or_else(|| ep.path.strip_prefix("bt://").and_then(normalize_bt_mac))
            {
                match try_bt_spp(&mac, &opts) {
                    Ok(conn) => return Ok(conn),
                    Err(e) => errors.push(format!("bt-spp {mac}: {e}")),
                }
            }
            continue;
        }
        let bauds = baud_list(&opts, ep.kind);
        for &baud in &bauds {
            debug!(path = %ep.path, baud, kind = %ep.kind, "try open serial adapter");
            match try_open_serial(&ep, baud, &opts) {
                Ok(transport) => {
                    info!(path = %ep.path, baud, kind = %ep.kind, "adapter ready");
                    return Ok(ConnectedAdapter {
                        transport: Box::new(transport),
                        endpoint: ep,
                        baud,
                    });
                }
                Err(e) => {
                    let msg = format!("{} @ {baud}: {e}", ep.path);
                    debug!("{msg}");
                    errors.push(msg);
                }
            }
        }
    }

    if errors.is_empty() {
        return Err(Error::NoAdapter);
    }
    Err(Error::AdapterInit(format!(
        "no working adapter (tried USB and/or Bluetooth).\n{}",
        errors.join("\n")
    )))
}

fn try_bt_spp(mac: &str, opts: &ConnectOptions) -> Result<ConnectedAdapter> {
    let mac = normalize_bt_mac(mac)
        .ok_or_else(|| Error::AdapterInit(format!("invalid Bluetooth MAC: {mac}")))?;
    let timeout = opts.timeout.max(Duration::from_millis(4_000));
    let mut transport = BtSppTransport::new(&mac, opts.rfcomm_channel, timeout, opts.default_bus)?;
    transport.open()?;
    if !opts.skip_init {
        transport.init()?;
    }
    info!(%mac, channel = opts.rfcomm_channel, "Bluetooth SPP adapter ready");
    let endpoint = AdapterEndpoint {
        path: format!("bt://{mac}"),
        kind: LinkKind::BluetoothSpp,
        description: "Bluetooth SPP (native RFCOMM socket)".into(),
        bt_mac: Some(mac),
    };
    Ok(ConnectedAdapter {
        transport: Box::new(transport),
        endpoint,
        baud: 0,
    })
}

fn try_open_serial(ep: &AdapterEndpoint, baud: u32, opts: &ConnectOptions) -> Result<ElmTransport> {
    let timeout = if ep.kind == LinkKind::BluetoothSpp {
        opts.timeout.max(Duration::from_millis(4_000))
    } else {
        opts.timeout
    };

    let config = ElmConfig {
        path: ep.path.clone(),
        baud,
        timeout,
        default_bus: opts.default_bus,
        link: ep.kind,
    };
    let mut transport = ElmTransport::new(config);
    transport.open()?;
    if !opts.skip_init {
        transport.init()?;
    }
    Ok(transport)
}

fn resolve_candidates(opts: &ConnectOptions) -> Result<Vec<AdapterEndpoint>> {
    if let Some(path) = &opts.path {
        if path.starts_with("bt://") {
            return Ok(vec![AdapterEndpoint {
                path: path.clone(),
                kind: LinkKind::BluetoothSpp,
                description: "Bluetooth SPP".into(),
                bt_mac: path.strip_prefix("bt://").and_then(normalize_bt_mac),
            }]);
        }
        return Ok(vec![AdapterEndpoint::from_path(path)]);
    }

    let mut all = discover_adapters()?;
    match opts.prefer {
        LinkPrefer::Usb => all.retain(|e| e.kind == LinkKind::UsbSerial),
        LinkPrefer::Bluetooth => {
            all.retain(|e| e.kind == LinkKind::BluetoothSpp || e.path.starts_with("bt://"))
        }
        LinkPrefer::Auto => {}
    }

    sort_endpoints(&mut all, opts.prefer);
    Ok(all)
}

fn baud_list(opts: &ConnectOptions, kind: LinkKind) -> Vec<u32> {
    if let Some(b) = opts.baud {
        return vec![b];
    }
    kind.default_bauds().to_vec()
}

fn sort_endpoints(list: &mut [AdapterEndpoint], prefer: LinkPrefer) {
    list.sort_by(|a, b| {
        let rank = |e: &AdapterEndpoint| -> u8 {
            match (prefer, e.kind) {
                (LinkPrefer::Usb, LinkKind::UsbSerial)
                | (LinkPrefer::Bluetooth, LinkKind::BluetoothSpp)
                | (LinkPrefer::Auto, LinkKind::UsbSerial) => 0,
                (LinkPrefer::Auto, LinkKind::BluetoothSpp) => 1,
                (LinkPrefer::Usb, LinkKind::BluetoothSpp)
                | (LinkPrefer::Bluetooth, LinkKind::UsbSerial) => 2,
                (_, LinkKind::Other) => 3,
            }
        };
        rank(a).cmp(&rank(b)).then_with(|| a.path.cmp(&b.path))
    });
}

fn is_candidate(ep: &AdapterEndpoint) -> bool {
    match ep.kind {
        LinkKind::UsbSerial | LinkKind::BluetoothSpp => true,
        LinkKind::Other => {
            let n = ep.path.to_ascii_lowercase();
            n.contains("obd") || n.contains("serial")
        }
    }
}

fn push_unique(out: &mut Vec<AdapterEndpoint>, ep: AdapterEndpoint) {
    if out.iter().any(|e| e.path == ep.path) {
        return;
    }
    out.push(ep);
}

struct BtHint {
    mac: String,
    name: String,
}

/// Best-effort list of paired Bluetooth devices via `bluetoothctl`.
fn bluetooth_paired_hints() -> Vec<BtHint> {
    let output = Command::new("bluetoothctl")
        .args(["devices", "Paired"])
        .output()
        .or_else(|_| Command::new("bluetoothctl").arg("devices").output());

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut hints = Vec::new();
    for line in text.lines() {
        // Device AA:BB:CC:DD:EE:FF Name here
        let line = line.trim();
        let rest = line.strip_prefix("Device ").unwrap_or(line);
        let mut parts = rest.splitn(2, char::is_whitespace);
        let mac_raw = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("").trim().to_string();
        if let Some(mac) = normalize_bt_mac(mac_raw) {
            let lname = name.to_ascii_lowercase();
            // Prefer names that look like OBD adapters; still list all if few.
            // Only list names that look like OBD adapters (not mice/keyboards).
            let obd_like = lname.contains("obd")
                || lname.contains("elm")
                || lname.contains("vgate")
                || lname.contains("obdlink")
                || lname.contains("veepen")
                || lname.contains("carista")
                || lname.contains("konnwei")
                || lname.contains("vlinker")
                || lname.contains("bies")
                || lname.contains("obdii")
                || lname.contains("obd2");
            if obd_like {
                hints.push(BtHint { mac, name });
            }
        }
    }
    hints
}

/// Bind RFCOMM for a Bluetooth MAC. Returns `/dev/rfcomm{index}`.
pub fn ensure_rfcomm(mac: &str, index: u8, channel: u8) -> Result<String> {
    let mac = normalize_bt_mac(mac)
        .ok_or_else(|| Error::AdapterInit(format!("invalid Bluetooth MAC: {mac}")))?;
    let path = format!("/dev/rfcomm{index}");

    if Path::new(&path).exists() {
        debug!(%path, "rfcomm device already present");
        return Ok(path);
    }

    info!(%mac, %path, channel, "bind RFCOMM");
    // Prefer user-owned bind; fall back to sudo if needed.
    let bind_args = [
        "bind".to_string(),
        index.to_string(),
        mac.clone(),
        channel.to_string(),
    ];

    let status = Command::new("rfcomm").args(&bind_args).status();
    match status {
        Ok(s) if s.success() => {
            if Path::new(&path).exists() {
                return Ok(path);
            }
        }
        Ok(s) => {
            warn!(code = ?s.code(), "rfcomm bind failed; try sudo");
        }
        Err(e) => {
            warn!(error = %e, "rfcomm not available");
        }
    }

    let sudo = Command::new("sudo")
        .arg("rfcomm")
        .args(&bind_args)
        .status()
        .map_err(|e| {
            Error::AdapterInit(format!(
                "rfcomm bind failed for {mac}: {e}. Pair the adapter, then run: sudo rfcomm bind {index} {mac} {channel}"
            ))
        })?;

    if !sudo.success() || !Path::new(&path).exists() {
        return Err(Error::AdapterInit(format!(
            "could not create {path} for {mac}. Pair with bluetoothctl, then: sudo rfcomm bind {index} {mac} {channel}"
        )));
    }

    Ok(path)
}

/// Release an RFCOMM binding (best effort).
pub fn release_rfcomm(index: u8) -> Result<()> {
    let path = format!("/dev/rfcomm{index}");
    let _ = Command::new("rfcomm")
        .args(["release", &index.to_string()])
        .status();
    let _ = Command::new("sudo")
        .args(["rfcomm", "release", &index.to_string()])
        .status();
    if Path::new(&path).exists() {
        warn!(%path, "rfcomm still present after release");
    }
    Ok(())
}

/// Format endpoints for CLI listing.
pub fn format_endpoint_list(endpoints: &[AdapterEndpoint]) -> String {
    if endpoints.is_empty() {
        return "No adapters found.\n\
Wire: plug USB OBD and check /dev/ttyUSB* or /dev/ttyACM*.\n\
Bluetooth: pair the adapter, then: sudo rfcomm bind 0 <MAC>\n\
  or: obdtui --bt-mac <MAC>"
            .into();
    }
    let mut lines = Vec::new();
    for e in endpoints {
        let mac = e
            .bt_mac
            .as_ref()
            .map(|m| format!(" mac={m}"))
            .unwrap_or_default();
        lines.push(format!(
            "{:<18}  {:<10}  {}{}",
            e.path, e.kind, e.description, mac
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_prefers_usb_in_auto() {
        let mut list = vec![
            AdapterEndpoint::from_path("/dev/rfcomm0"),
            AdapterEndpoint::from_path("/dev/ttyUSB0"),
        ];
        sort_endpoints(&mut list, LinkPrefer::Auto);
        assert_eq!(list[0].path, "/dev/ttyUSB0");
        assert_eq!(list[1].path, "/dev/rfcomm0");
    }

    #[test]
    fn sort_prefers_bt_when_asked() {
        let mut list = vec![
            AdapterEndpoint::from_path("/dev/ttyUSB0"),
            AdapterEndpoint::from_path("/dev/rfcomm0"),
        ];
        sort_endpoints(&mut list, LinkPrefer::Bluetooth);
        assert_eq!(list[0].path, "/dev/rfcomm0");
    }

    #[test]
    fn classify_in_from_path() {
        let e = AdapterEndpoint::from_path("/dev/rfcomm1");
        assert_eq!(e.kind, LinkKind::BluetoothSpp);
        assert_eq!(
            super::super::link::classify_path(&e.path),
            LinkKind::BluetoothSpp
        );
    }
}

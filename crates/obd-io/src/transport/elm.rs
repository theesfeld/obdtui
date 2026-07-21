//! ELM327 / STN serial transport.

use super::Transport;
use crate::bus::{AdapterCapabilities, BusTag};
use crate::error::{Error, Result};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Serial port configuration for ELM/STN adapters.
#[derive(Debug, Clone)]
pub struct ElmConfig {
    pub path: String,
    pub baud: u32,
    pub timeout: Duration,
    /// Force bus tag for captures when only one bus is visible.
    pub default_bus: BusTag,
}

impl Default for ElmConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            baud: 38400,
            timeout: Duration::from_millis(2_000),
            default_bus: BusTag::Hs,
        }
    }
}

/// Live ELM327/STN adapter over a USB serial port.
pub struct ElmTransport {
    config: ElmConfig,
    port: Option<Box<dyn SerialPort>>,
    caps: AdapterCapabilities,
    active_bus: BusTag,
}

impl ElmTransport {
    pub fn new(config: ElmConfig) -> Self {
        let active_bus = config.default_bus;
        Self {
            config,
            port: None,
            caps: AdapterCapabilities::default(),
            active_bus,
        }
    }

    /// Open the configured serial path without full OBD init.
    pub fn open(&mut self) -> Result<()> {
        if self.config.path.is_empty() {
            return Err(Error::NoAdapter);
        }
        let port = serialport::new(&self.config.path, self.config.baud)
            .timeout(self.config.timeout)
            .open()
            .map_err(Error::Serial)?;
        self.port = Some(port);
        Ok(())
    }

    fn port_mut(&mut self) -> Result<&mut Box<dyn SerialPort>> {
        self.port
            .as_mut()
            .ok_or_else(|| Error::AdapterInit("serial port is not open".into()))
    }

    fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        let port = self.port_mut()?;
        port.write_all(data)?;
        port.flush()?;
        Ok(())
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        let timeout = self.config.timeout;
        let start = Instant::now();
        let mut buf = Vec::with_capacity(256);
        let mut byte = [0u8; 1];

        loop {
            if start.elapsed() > timeout {
                if buf.is_empty() {
                    return Err(Error::Timeout);
                }
                break;
            }
            let port = self.port_mut()?;
            match port.read(&mut byte) {
                Ok(0) => continue,
                Ok(_) => {
                    if byte[0] == b'>' {
                        break;
                    }
                    // Skip nulls from some clones.
                    if byte[0] != 0 {
                        buf.push(byte[0]);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if !buf.is_empty() {
                        break;
                    }
                    continue;
                }
                Err(e) => return Err(Error::Io(e)),
            }
        }

        let text = String::from_utf8_lossy(&buf).to_string();
        Ok(clean_response(&text))
    }

    fn at_command(&mut self, cmd: &str) -> Result<String> {
        debug!(command = cmd, "AT command");
        let mut line = cmd.as_bytes().to_vec();
        if !line.ends_with(b"\r") {
            line.push(b'\r');
        }
        self.write_raw(&line)?;
        let resp = self.read_until_prompt()?;
        debug!(response = %resp, "AT response");
        Ok(resp)
    }

    fn detect_ms_can_hint(identity: &str, stdi: &str) -> bool {
        let blob = format!("{identity} {stdi}").to_ascii_uppercase();
        // STN / OBDLink class often expose multi-bus; cheap ELM clones do not.
        blob.contains("STN")
            || blob.contains("OBDLINK")
            || blob.contains("SCANTOOL")
            || blob.contains("STN11")
    }
}

impl Transport for ElmTransport {
    fn name(&self) -> &str {
        &self.config.path
    }

    fn capabilities(&self) -> &AdapterCapabilities {
        &self.caps
    }

    fn active_bus(&self) -> BusTag {
        self.active_bus
    }

    fn select_bus(&mut self, bus: BusTag) -> Result<()> {
        if bus == BusTag::Ms && !self.caps.ms_can {
            return Err(Error::Protocol(
                "adapter does not support MS-CAN in this build".into(),
            ));
        }
        // STN dual-bus selection is adapter-specific (SPA/SPB, headers).
        // Phase 0 records the tag; advanced switch commands come with hardware proof.
        self.active_bus = bus;
        Ok(())
    }

    fn init(&mut self) -> Result<()> {
        if self.port.is_none() {
            self.open()?;
        }

        // Drain leftover input.
        if let Ok(port) = self.port_mut() {
            let _ = port.clear(serialport::ClearBuffer::All);
        }

        // Soft reset and basic ELM setup.
        let _ = self.at_command("ATZ");
        std::thread::sleep(Duration::from_millis(500));
        // After ATZ some adapters re-print banner without waiting; send CR.
        let _ = self.write_raw(b"\r");
        let _ = self.read_until_prompt();

        let _ = self.at_command("ATE0"); // echo off
        let _ = self.at_command("ATL0"); // linefeeds off
        let _ = self.at_command("ATS0"); // spaces off (some clones ignore)
        let _ = self.at_command("ATH0"); // headers off for simple J1979
        let _ = self.at_command("ATSP0"); // auto protocol

        let identity = self.at_command("ATI").unwrap_or_default();
        let stdi = self.at_command("STDI").unwrap_or_else(|_| String::new());
        if stdi.contains("?") || stdi.is_empty() {
            // Not STN — ignore.
        }

        let protocol = self.at_command("ATDP").unwrap_or_default();

        let stn = identity.to_ascii_uppercase().contains("STN")
            || (!stdi.is_empty() && !stdi.contains('?'));
        let ms_can = Self::detect_ms_can_hint(&identity, &stdi);

        self.caps = AdapterCapabilities {
            elm_compatible: !identity.is_empty(),
            stn,
            ms_can,
            identity: identity.trim().to_string(),
            protocol: protocol.trim().to_string(),
        };

        if !self.caps.elm_compatible {
            warn!("adapter identity empty after init");
            return Err(Error::AdapterInit("adapter did not respond to ATI".into()));
        }

        self.active_bus = self.config.default_bus;
        Ok(())
    }

    fn transact(&mut self, command: &str) -> Result<String> {
        let cmd = command.trim();
        if cmd.is_empty() {
            return Err(Error::Protocol("empty command".into()));
        }
        self.at_command(cmd)
    }
}

/// List likely OBD serial devices: USB-serial and Bluetooth RFCOMM.
pub fn discover_serial_ports() -> Result<Vec<String>> {
    let ports = serialport::available_ports().map_err(Error::Serial)?;
    let mut out: Vec<String> = ports
        .into_iter()
        .map(|p| p.port_name)
        .filter(|name| is_likely_obd_port(name))
        .collect();

    // Also scan common RFCOMM device nodes (may not appear in serialport enum).
    for n in 0..8 {
        let path = format!("/dev/rfcomm{n}");
        if std::path::Path::new(&path).exists() && !out.iter().any(|p| p == &path) {
            out.push(path);
        }
    }

    out.sort();
    if out.is_empty() {
        // Fall back to all ports so the user can pick.
        let ports = serialport::available_ports().map_err(Error::Serial)?;
        out = ports.into_iter().map(|p| p.port_name).collect();
        out.sort();
    }
    Ok(out)
}

fn is_likely_obd_port(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("ttyusb")
        || n.contains("ttyacm")
        || n.contains("cu.usb")
        || n.contains("usbserial")
        || n.contains("rfcomm")
        || n.contains("obd")
        || n.contains("serial")
}

fn clean_response(raw: &str) -> String {
    raw.chars()
        .filter(|c| *c != '\r')
        .collect::<String>()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| *l != "SEARCHING...")
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_searching() {
        let s = clean_response("SEARCHING...\r\n410C1AF8\r\n");
        assert_eq!(s, "410C1AF8");
    }

    #[test]
    fn ms_can_hint_stn() {
        assert!(ElmTransport::detect_ms_can_hint("STN1170 v5.6.6", ""));
        assert!(!ElmTransport::detect_ms_can_hint("ELM327 v1.5", ""));
    }
}

//! Bluetooth classic SPP via Linux RFCOMM sockets (no root `/dev/rfcomm` bind).

use super::Transport;
use crate::bus::{AdapterCapabilities, BusTag, LinkKind};
use crate::error::{Error, Result};
use crate::transport::link::normalize_bt_mac;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};
use tracing::{debug, info};

// Linux bluetooth constants
const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_RFCOMM: libc::c_int = 3;

#[repr(C)]
#[derive(Clone, Copy)]
struct BdAddr {
    b: [u8; 6],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockaddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: BdAddr,
    rc_channel: u8,
}

/// Parse `AA:BB:CC:DD:EE:FF` into BlueZ little-endian byte order for `bdaddr_t`.
fn parse_bdaddr(mac: &str) -> Result<BdAddr> {
    let norm = normalize_bt_mac(mac)
        .ok_or_else(|| Error::AdapterInit(format!("invalid Bluetooth MAC: {mac}")))?;
    let mut parts = [0u8; 6];
    for (i, p) in norm.split(':').enumerate() {
        parts[i] = u8::from_str_radix(p, 16)
            .map_err(|e| Error::AdapterInit(format!("bad MAC byte: {e}")))?;
    }
    // BlueZ stores BDADDR in reverse order.
    parts.reverse();
    Ok(BdAddr { b: parts })
}

/// Live ELM/STN over Bluetooth classic RFCOMM (SPP).
pub struct BtSppTransport {
    name: String,
    mac: String,
    channel: u8,
    fd: Option<OwnedFd>,
    caps: AdapterCapabilities,
    active_bus: BusTag,
    timeout: Duration,
    default_bus: BusTag,
}

impl BtSppTransport {
    pub fn new(
        mac: impl Into<String>,
        channel: u8,
        timeout: Duration,
        default_bus: BusTag,
    ) -> Result<Self> {
        let mac_raw = mac.into();
        let mac = normalize_bt_mac(&mac_raw)
            .ok_or_else(|| Error::AdapterInit(format!("invalid Bluetooth MAC: {mac_raw}")))?;
        Ok(Self {
            name: format!("bt://{mac}"),
            mac,
            channel,
            fd: None,
            caps: AdapterCapabilities {
                link: LinkKind::BluetoothSpp,
                ..AdapterCapabilities::default()
            },
            active_bus: default_bus,
            timeout,
            default_bus,
        })
    }

    pub fn open(&mut self) -> Result<()> {
        if self.fd.is_some() {
            return Ok(());
        }
        let addr = parse_bdaddr(&self.mac)?;
        let fd = unsafe {
            libc::socket(
                AF_BLUETOOTH,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                BTPROTO_RFCOMM,
            )
        };
        if fd < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };

        // Set receive timeout
        let tv = libc::timeval {
            tv_sec: self.timeout.as_secs() as libc::time_t,
            tv_usec: self.timeout.subsec_micros() as libc::suseconds_t,
        };
        unsafe {
            libc::setsockopt(
                owned.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const _ as *const libc::c_void,
                std::mem::size_of_val(&tv) as libc::socklen_t,
            );
            libc::setsockopt(
                owned.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                &tv as *const _ as *const libc::c_void,
                std::mem::size_of_val(&tv) as libc::socklen_t,
            );
        }

        let sa = SockaddrRc {
            rc_family: AF_BLUETOOTH as libc::sa_family_t,
            rc_bdaddr: addr,
            rc_channel: self.channel,
        };

        // Non-blocking connect: BlueZ often returns EINPROGRESS.
        let flags = unsafe { libc::fcntl(owned.as_raw_fd(), libc::F_GETFL) };
        if flags >= 0 {
            unsafe {
                libc::fcntl(owned.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let rc = unsafe {
            libc::connect(
                owned.as_raw_fd(),
                &sa as *const _ as *const libc::sockaddr,
                std::mem::size_of_val(&sa) as libc::socklen_t,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            let einprogress = err.raw_os_error() == Some(libc::EINPROGRESS)
                || err.kind() == std::io::ErrorKind::WouldBlock;
            if !einprogress {
                return Err(Error::AdapterInit(format!(
                    "RFCOMM connect to {} channel {} failed: {err}. Pair the adapter first (bluetoothctl).",
                    self.mac, self.channel
                )));
            }
            // Wait until socket is writable or timeout.
            let deadline = Instant::now() + self.timeout.max(Duration::from_secs(8));
            loop {
                let mut pollfd = libc::pollfd {
                    fd: owned.as_raw_fd(),
                    events: libc::POLLOUT,
                    revents: 0,
                };
                let pret = unsafe { libc::poll(&mut pollfd, 1, 200) };
                if pret > 0 && (pollfd.revents & libc::POLLOUT) != 0 {
                    // Check SO_ERROR
                    let mut so_error: libc::c_int = 0;
                    let mut len = std::mem::size_of_val(&so_error) as libc::socklen_t;
                    let g = unsafe {
                        libc::getsockopt(
                            owned.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_ERROR,
                            &mut so_error as *mut _ as *mut libc::c_void,
                            &mut len,
                        )
                    };
                    if g == 0 && so_error == 0 {
                        break;
                    }
                    if g == 0 && so_error != 0 {
                        return Err(Error::AdapterInit(format!(
                            "RFCOMM connect to {} channel {} failed: {}",
                            self.mac,
                            self.channel,
                            std::io::Error::from_raw_os_error(so_error)
                        )));
                    }
                }
                if Instant::now() >= deadline {
                    return Err(Error::AdapterInit(format!(
                        "RFCOMM connect to {} channel {} timed out. Is the adapter paired and in range?",
                        self.mac, self.channel
                    )));
                }
            }
        }

        // Restore blocking mode for ELM IO.
        if flags >= 0 {
            unsafe {
                libc::fcntl(owned.as_raw_fd(), libc::F_SETFL, flags);
            }
        }

        info!(mac = %self.mac, channel = self.channel, "RFCOMM SPP connected");
        self.fd = Some(owned);
        Ok(())
    }

    fn with_file<R>(&mut self, f: impl FnOnce(&mut std::fs::File) -> Result<R>) -> Result<R> {
        let owned = self
            .fd
            .as_ref()
            .ok_or_else(|| Error::AdapterInit("RFCOMM not open".into()))?;
        // Duplicate fd so we don't drop the OwnedFd when File is dropped.
        let dup = unsafe { libc::dup(owned.as_raw_fd()) };
        if dup < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        let mut file = unsafe { std::fs::File::from_raw_fd(dup) };
        let result = f(&mut file);
        // File drop closes dup only.
        drop(file);
        result
    }

    fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        self.with_file(|file| {
            file.write_all(data)?;
            file.flush()?;
            Ok(())
        })
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        let timeout = self.timeout;
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
            let n = self.with_file(|file| match file.read(&mut byte) {
                Ok(n) => Ok(n),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    Ok(0)
                }
                Err(e) => Err(Error::Io(e)),
            })?;
            if n == 0 {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            if byte[0] == b'>' {
                break;
            }
            if byte[0] != 0 {
                buf.push(byte[0]);
            }
        }
        let text = String::from_utf8_lossy(&buf).to_string();
        Ok(clean_response(&text))
    }

    fn at_command(&mut self, cmd: &str) -> Result<String> {
        debug!(command = cmd, "BT AT command");
        let mut line = cmd.as_bytes().to_vec();
        if !line.ends_with(b"\r") {
            line.push(b'\r');
        }
        self.write_raw(&line)?;
        let resp = self.read_until_prompt()?;
        debug!(response = %resp, "BT AT response");
        Ok(resp)
    }
}

impl Drop for BtSppTransport {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            let raw = fd.into_raw_fd();
            unsafe {
                libc::shutdown(raw, libc::SHUT_RDWR);
                libc::close(raw);
            }
        }
    }
}

impl Transport for BtSppTransport {
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
        if bus == BusTag::Ms && !self.caps.ms_can {
            return Err(Error::Protocol(
                "adapter does not support MS-CAN in this build".into(),
            ));
        }
        self.active_bus = bus;
        Ok(())
    }

    fn init(&mut self) -> Result<()> {
        if self.fd.is_none() {
            self.open()?;
        }
        let settle = Duration::from_millis(1_200);
        let atz = self.at_command("ATZ").unwrap_or_default();
        std::thread::sleep(settle);
        let _ = self.write_raw(b"\r");
        let _ = self.read_until_prompt();

        let _ = self.at_command("ATE0");
        let _ = self.at_command("ATL0");
        let _ = self.at_command("ATS0");
        let _ = self.at_command("ATH0");
        // Adaptive timing for slow truck ECUs over Bluetooth.
        let _ = self.at_command("ATAT2");
        // Auto protocol; vehicle must be key-on / running.
        let _ = self.at_command("ATSP0");
        std::thread::sleep(Duration::from_millis(400));
        // Warm-up Mode 01 PID 00 (protocol lock). Keep attempts short — flaky bus is common.
        let prev_timeout = self.timeout;
        self.timeout = Duration::from_secs(8);
        let mut locked = false;
        for attempt in 1..=3 {
            let resp = self.at_command("0100").unwrap_or_default();
            let upper = resp.to_ascii_uppercase().replace(' ', "");
            if upper.contains("4100") || (upper.contains("41") && upper.len() >= 6) {
                locked = true;
                debug!(attempt, %resp, "protocol lock ok");
                break;
            }
            if attempt == 2 {
                let _ = self.at_command("ATSP6"); // ISO 15765-4 CAN 11/500
            }
            debug!(attempt, %resp, "protocol lock retry");
            std::thread::sleep(Duration::from_millis(300));
        }
        self.timeout = prev_timeout;
        if !locked {
            warn_protocol_not_locked();
        }

        let identity = sanitize_identity(&self.at_command("ATI").unwrap_or_default());
        let identity = if identity.is_empty() || is_bus_noise(&identity) {
            // Prefer ATZ banner (e.g. ELM327 v1.4b) when ATI is noisy.
            sanitize_identity(&atz)
        } else {
            identity
        };
        let stdi = self.at_command("STDI").unwrap_or_else(|_| String::new());
        let protocol = self.at_command("ATDP").unwrap_or_default();

        let stn = identity.to_ascii_uppercase().contains("STN")
            || identity.to_ascii_uppercase().contains("OBDLINK")
            || identity.to_ascii_uppercase().contains("ELM327")
            || (!stdi.is_empty() && !stdi.contains('?'));
        let ms_can = stn
            || identity.to_ascii_uppercase().contains("OBDLINK")
            || stdi.to_ascii_uppercase().contains("STN");

        self.caps = AdapterCapabilities {
            elm_compatible: !identity.is_empty() && !is_bus_noise(&identity),
            stn,
            ms_can,
            link: LinkKind::BluetoothSpp,
            identity: identity.trim().to_string(),
            protocol: protocol.trim().to_string(),
        };
        if !self.caps.elm_compatible {
            // Still mark compatible if ATZ showed ELM — vehicle data may still work.
            if atz.to_ascii_uppercase().contains("ELM") {
                self.caps.elm_compatible = true;
                self.caps.identity = sanitize_identity(&atz);
            }
        }
        if !self.caps.elm_compatible {
            return Err(Error::AdapterInit(
                "adapter did not respond to ATI over Bluetooth SPP".into(),
            ));
        }
        self.active_bus = self.default_bus;
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

fn clean_response(raw: &str) -> String {
    raw.chars()
        .filter(|c| *c != '\r')
        .collect::<String>()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !is_bus_noise(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_bus_noise(line: &str) -> bool {
    let u = line.to_ascii_uppercase();
    matches!(u.as_str(), "SEARCHING..." | "STOPPED" | "ERROR" | "?")
        || u.contains("UNABLE TO CONNECT")
        || u.contains("BUS INIT")
        || u.contains("CAN ERROR")
}

fn sanitize_identity(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !is_bus_noise(l) && *l != "OK")
        .collect::<Vec<_>>()
        .join(" ")
}

fn warn_protocol_not_locked() {
    tracing::warn!(
        "OBD protocol did not lock on 0100; keep ignition on and retry if data is empty"
    );
}

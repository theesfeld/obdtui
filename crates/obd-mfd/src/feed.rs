//! Background OBD feed for the MFD window.

use crate::Args;
use anyhow::{Context, Result};
use obd_io::{
    connect, generic_profile, priority_pids, ConnectOptions, LiveValue, ReplayTransport,
    VehicleSession,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Telemetry {
    pub values: HashMap<String, f64>,
    pub units: HashMap<String, String>,
    pub vin: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub ticks: u64,
}

pub struct ObdFeed {
    stop: Arc<AtomicBool>,
    tele: Arc<Mutex<Telemetry>>,
    _join: Option<JoinHandle<()>>,
}

impl ObdFeed {
    pub fn start(args: &Args) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let tele = Arc::new(Mutex::new(Telemetry {
            status: "connecting…".into(),
            ..Default::default()
        }));

        let stop_t = Arc::clone(&stop);
        let tele_t = Arc::clone(&tele);

        let replay = args.replay.clone();
        let port = args.port.clone();
        let baud = args.baud;
        let prefer = args.connect_prefer()?;
        let bt_mac = args.bt_mac.clone();
        let channel = args.rfcomm_channel;
        let timeout = args.timeout();

        let join = thread::Builder::new()
            .name("obd-feed".into())
            .spawn(move || {
                let session = match open_session(
                    replay,
                    port,
                    baud,
                    prefer,
                    bt_mac,
                    channel,
                    timeout,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        if let Ok(mut t) = tele_t.lock() {
                            t.error = Some(e.to_string());
                            t.status = "connect failed".into();
                        }
                        return;
                    }
                };
                run_loop(session, stop_t, tele_t);
            })
            .context("spawn feed thread")?;

        Ok(Self {
            stop,
            tele,
            _join: Some(join),
        })
    }

    pub fn snapshot(&self) -> Telemetry {
        self.tele.lock().map(|t| t.clone()).unwrap_or_default()
    }
}

impl Drop for ObdFeed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self._join.take() {
            let _ = j.join();
        }
    }
}

fn open_session(
    replay: Option<std::path::PathBuf>,
    port: Option<String>,
    baud: Option<u32>,
    prefer: obd_io::LinkPrefer,
    bt_mac: Option<String>,
    channel: u8,
    timeout: Duration,
) -> Result<VehicleSession> {
    let software = format!("obd-mfd {}", env!("CARGO_PKG_VERSION"));
    let profile = generic_profile();

    if let Some(path) = replay {
        let transport = ReplayTransport::from_path(&path)
            .with_context(|| format!("replay {}", path.display()))?;
        let mut session = VehicleSession::new(Box::new(transport), profile, software);
        session.init()?;
        let _ = session.read_vin();
        let _ = session.probe_supported_pids();
        return Ok(session);
    }

    let opts = ConnectOptions {
        path: port,
        baud,
        prefer,
        bt_mac,
        rfcomm_index: 0,
        rfcomm_channel: channel,
        timeout,
        default_bus: obd_io::BusTag::Hs,
        skip_init: false,
    };
    let connected = connect(opts).context("OBD connect")?;
    let mut session = VehicleSession::new(connected.transport, profile, software);
    let _ = session.read_vin();
    let _ = session.probe_supported_pids();
    Ok(session)
}

fn run_loop(mut session: VehicleSession, stop: Arc<AtomicBool>, tele: Arc<Mutex<Telemetry>>) {
    {
        if let Ok(mut t) = tele.lock() {
            t.vin = session.vin.clone();
            t.status = format!("live · {}", session.transport_name());
        }
    }

    // Weighted priority for HUD: rpm/speed/throttle dominate.
    let mut order: Vec<u8> = Vec::new();
    for &p in priority_pids() {
        order.push(p);
        if matches!(p, 0x0C | 0x0D | 0x11 | 0x04) {
            order.push(p);
            order.push(p);
        }
    }
    if order.is_empty() {
        order = vec![0x0C, 0x0D, 0x04, 0x11, 0x05, 0x0C];
    }

    let mut i = 0usize;
    while !stop.load(Ordering::Relaxed) {
        let pid = order[i % order.len()];
        i = i.wrapping_add(1);

        match session.read_pid(pid) {
            Ok(v) => apply_value(&tele, &v, &session),
            Err(e) => {
                if let Ok(mut t) = tele.lock() {
                    t.error = Some(e.to_string());
                }
            }
        }
        // Minimal yield; BT RTT is the real limiter.
        thread::sleep(Duration::from_millis(2));
    }
}

fn apply_value(tele: &Arc<Mutex<Telemetry>>, v: &LiveValue, session: &VehicleSession) {
    if let Ok(mut t) = tele.lock() {
        t.values.insert(v.name.clone(), v.value);
        t.units.insert(v.name.clone(), v.unit.clone());
        t.vin = session.vin.clone();
        t.ticks = t.ticks.wrapping_add(1);
        t.error = None;
        t.status = format!("live · {}", session.transport_name());
    }
}

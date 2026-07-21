//! obdtui — OBD-II diagnostic terminal UI.

mod app;
mod ui;

use anyhow::{bail, Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use obd_io::{
    connect, discover_adapters, format_endpoint_list, generic_profile, load_profiles_dir,
    ConnectOptions, LinkPrefer, ReplayTransport, VehicleSession,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::app::{App, Tab};

#[derive(Parser, Debug)]
#[command(
    name = "obdtui",
    about = "OBD-II diagnostic TUI. USB serial and Bluetooth SPP. Default mode is read-only.",
    version,
    long_about = "Connect to a USB OBD-II adapter or Bluetooth classic SPP adapter,\n\
or replay a capture session.\n\
Default mode is read-only. Clear DTCs only with --allow-writes.\n\
0.x releases may change interfaces."
)]
struct Args {
    /// Serial port path (USB: /dev/ttyUSB0, BT: /dev/rfcomm0). Auto-detect when omitted.
    #[arg(short, long, env = "OBDTUI_PORT")]
    port: Option<String>,

    /// Serial baud rate. When omitted, try common rates for the link type.
    #[arg(long, env = "OBDTUI_BAUD")]
    baud: Option<u32>,

    /// Prefer link type when auto-detecting: auto | usb | bluetooth
    #[arg(long, default_value = "auto", env = "OBDTUI_PREFER")]
    prefer: String,

    /// Bluetooth MAC (classic SPP). Binds RFCOMM when needed (may need sudo).
    #[arg(long, env = "OBDTUI_BT_MAC")]
    bt_mac: Option<String>,

    /// RFCOMM device index for --bt-mac (default 0 → /dev/rfcomm0)
    #[arg(long, default_value_t = 0)]
    rfcomm_index: u8,

    /// RFCOMM channel (default 1 for most ELM sticks)
    #[arg(long, default_value_t = 1)]
    rfcomm_channel: u8,

    /// Replay a capture directory instead of live hardware
    #[arg(long)]
    replay: Option<PathBuf>,

    /// Directory for vehicle profile YAML files
    #[arg(long, default_value = "profiles")]
    profiles: PathBuf,

    /// Profile id to load (default: generic_j1979)
    #[arg(long, default_value = "generic_j1979")]
    profile: String,

    /// Directory root for new captures
    #[arg(long, default_value = "captures")]
    capture_dir: PathBuf,

    /// Allow write operations such as clear DTCs
    #[arg(long, default_value_t = false)]
    allow_writes: bool,

    /// List USB and Bluetooth endpoints and exit
    #[arg(long)]
    list_ports: bool,

    /// Write the sample capture fixture and exit
    #[arg(long)]
    write_sample: Option<PathBuf>,

    /// Connect, print VIN / live sample / DTC count, exit (no TUI)
    #[arg(long)]
    probe: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(io::stderr)
        .init();

    let args = Args::parse();

    if args.list_ports {
        match discover_adapters() {
            Ok(endpoints) => {
                println!("{}", format_endpoint_list(&endpoints));
            }
            Err(e) => bail!("port scan failed: {e}"),
        }
        return Ok(());
    }

    if let Some(out) = args.write_sample {
        let mut session = obd_sim::sample_session();
        session.save(&out)?;
        println!("Wrote sample capture to {}", out.display());
        return Ok(());
    }

    let profile = load_profile(&args.profiles, &args.profile)?;
    let mut session = build_session(&args, profile)?;
    session.allow_writes = args.allow_writes;

    // Live connect already runs init inside `connect()`. Replay still needs init.
    if args.replay.is_some() {
        session.init().context("adapter init failed")?;
    }

    if args.probe {
        return run_probe(&mut session);
    }

    // Best-effort identity reads (replay and live).
    let mut boot_log = Vec::new();
    match session.read_vin() {
        Ok(v) => boot_log.push(format!("VIN {v}")),
        Err(e) => boot_log.push(format!("VIN not available: {e}")),
    }
    match session.probe_supported_pids() {
        Ok(p) => boot_log.push(format!("Supported PIDs (block 0): {}", p.len())),
        Err(e) => boot_log.push(format!("PID support probe failed: {e}")),
    }

    let mut app = App::new(session, args.capture_dir.clone());
    for line in boot_log {
        app.push_log(line);
    }
    let link = app.session.capabilities().link;
    let ms_can = app.session.capabilities().ms_can;
    app.push_log(format!("Link: {link}"));
    if ms_can {
        app.push_log("Adapter reports MS-CAN capability.".into());
    } else {
        app.push_log("MS-CAN: not available on this adapter.".into());
    }
    if args.allow_writes {
        app.push_log("Writes enabled (clear DTC allowed).".into());
    } else {
        app.push_log("Read-only mode.".into());
    }

    run_tui(&mut app)
}

fn run_probe(session: &mut VehicleSession) -> Result<()> {
    println!("transport: {}", session.transport_name());
    println!("caps: {}", session.capabilities().summary());
    match session.read_vin() {
        Ok(v) => println!("VIN: {v}"),
        Err(e) => println!("VIN: (not available) {e}"),
    }
    match session.probe_supported_pids() {
        Ok(p) => println!("supported PIDs (block 0): {}", p.len()),
        Err(e) => println!("PID support probe: {e}"),
    }
    match session.poll_dashboard() {
        Ok(vals) => {
            for v in vals {
                println!("  {} = {:.2} {}", v.name, v.value, v.unit);
            }
        }
        Err(e) => println!("live poll: {e}"),
    }
    match session.read_dtcs() {
        Ok(dtcs) => {
            println!("DTCs: {}", dtcs.len());
            for d in dtcs {
                println!("  {} ({:?})", d.code, d.status);
            }
        }
        Err(e) => println!("DTC read: {e}"),
    }
    Ok(())
}

fn load_profile(dir: &PathBuf, id: &str) -> Result<obd_io::VehicleProfile> {
    let profiles = load_profiles_dir(dir).unwrap_or_default();
    if let Some(p) = profiles.into_iter().find(|p| p.id == id) {
        return Ok(p);
    }
    Ok(generic_profile())
}

fn build_session(args: &Args, profile: obd_io::VehicleProfile) -> Result<VehicleSession> {
    let software = format!("obdtui {}", env!("CARGO_PKG_VERSION"));
    if let Some(path) = &args.replay {
        let transport = ReplayTransport::from_path(path)
            .with_context(|| format!("load replay from {}", path.display()))?;
        return Ok(VehicleSession::new(Box::new(transport), profile, software));
    }

    let prefer: LinkPrefer = args
        .prefer
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let opts = ConnectOptions {
        path: args.port.clone(),
        baud: args.baud,
        prefer,
        bt_mac: args.bt_mac.clone(),
        rfcomm_index: args.rfcomm_index,
        rfcomm_channel: args.rfcomm_channel,
        timeout: Duration::from_millis(2_500),
        default_bus: obd_io::BusTag::Hs,
        skip_init: false,
    };

    let connected = connect(opts).context("connect to OBD adapter (USB or Bluetooth)")?;
    if connected.baud == 0 {
        eprintln!(
            "Connected: {} ({})",
            connected.endpoint.path, connected.endpoint.kind
        );
    } else {
        eprintln!(
            "Connected: {} ({}) @ {} baud",
            connected.endpoint.path, connected.endpoint.kind, connected.baud
        );
    }

    Ok(VehicleSession::new(connected.transport, profile, software))
}

fn run_tui(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick = Duration::from_millis(200);
    let mut last_poll = Instant::now() - Duration::from_secs(10);
    let poll_every = Duration::from_millis(800);

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| ui::draw(f, app))?;

            if app.live_poll && last_poll.elapsed() >= poll_every {
                app.poll_live();
                last_poll = Instant::now();
            }

            if event::poll(tick)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.prev_tab(),
                        KeyCode::Char('1') => app.tab = Tab::Live,
                        KeyCode::Char('2') => app.tab = Tab::Dtc,
                        KeyCode::Char('3') => app.tab = Tab::Log,
                        KeyCode::Char('4') => app.tab = Tab::Help,
                        KeyCode::Char('r') => app.refresh_dtcs(),
                        KeyCode::Char('p') => {
                            app.live_poll = !app.live_poll;
                            app.push_log(if app.live_poll {
                                "Live poll: on".into()
                            } else {
                                "Live poll: off".into()
                            });
                        }
                        KeyCode::Char('c') => app.toggle_capture(),
                        KeyCode::Char('x') => app.try_clear_dtcs(),
                        KeyCode::Char('h') => app.tab = Tab::Help,
                        KeyCode::Char('b') => app.cycle_bus(),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

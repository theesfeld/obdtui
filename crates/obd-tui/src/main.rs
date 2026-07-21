//! obdtui — OBD-II diagnostic terminal UI (F1–F6 ramp).

mod app;
mod gauges;
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
use std::time::Duration;

use crate::app::{App, Tab};

#[derive(Parser, Debug)]
#[command(
    name = "obdtui",
    about = "OBD-II TUI: full Mode 01 capture, vector gauges, MFD shell.",
    version,
    long_about = "USB serial or Bluetooth SPP. Default mode is read-only.\n\
Full capture polls all supported Mode 01 PIDs.\n\
0.x releases may change interfaces."
)]
struct Args {
    #[arg(short, long, env = "OBDTUI_PORT")]
    port: Option<String>,

    #[arg(long, env = "OBDTUI_BAUD")]
    baud: Option<u32>,

    #[arg(long, default_value = "auto", env = "OBDTUI_PREFER")]
    prefer: String,

    #[arg(long, env = "OBDTUI_BT_MAC")]
    bt_mac: Option<String>,

    #[arg(long, default_value_t = 0)]
    rfcomm_index: u8,

    #[arg(long, default_value_t = 1)]
    rfcomm_channel: u8,

    #[arg(long)]
    replay: Option<PathBuf>,

    #[arg(long, default_value = "profiles")]
    profiles: PathBuf,

    #[arg(long, default_value = "generic_j1979")]
    profile: String,

    #[arg(long, default_value = "captures")]
    capture_dir: PathBuf,

    #[arg(long, default_value_t = false)]
    allow_writes: bool,

    /// Dashboard-only capture (disable full Mode 01 enumeration poll)
    #[arg(long, default_value_t = false)]
    no_full_capture: bool,

    #[arg(long)]
    list_ports: bool,

    #[arg(long)]
    write_sample: Option<PathBuf>,

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
            Ok(endpoints) => println!("{}", format_endpoint_list(&endpoints)),
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
    session.full_capture = !args.no_full_capture;

    if args.replay.is_some() {
        session.init().context("adapter init failed")?;
    }

    if args.probe {
        return run_probe(&mut session);
    }

    let mut boot_log = Vec::new();
    match session.read_vin() {
        Ok(v) => boot_log.push(format!("VIN {v}")),
        Err(e) => boot_log.push(format!("VIN not available: {e}")),
    }
    match session.probe_supported_pids() {
        Ok(p) => boot_log.push(format!("Supported Mode 01 PIDs: {}", p.len())),
        Err(e) => boot_log.push(format!("PID support probe failed: {e}")),
    }
    if let Ok(cal) = session.read_calid() {
        boot_log.push(format!("CALID {cal}"));
    }

    let mut app = App::new(session, args.capture_dir.clone());
    for line in boot_log {
        app.push_log(line);
    }
    let link = app.session.capabilities().link;
    let ms_can = app.session.capabilities().ms_can;
    app.push_log(format!("Link: {link}"));
    app.push_log(if ms_can {
        "MS-CAN capable adapter.".into()
    } else {
        "MS-CAN: not advertised.".into()
    });
    app.push_log(if app.session.full_capture {
        "Full Mode 01 capture enabled (press c).".into()
    } else {
        "Dashboard-only capture.".into()
    });
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
        Ok(p) => {
            println!("supported Mode 01 PIDs: {}", p.len());
            print!("  ");
            for (i, pid) in p.iter().enumerate() {
                if i > 0 && i % 16 == 0 {
                    print!("\n  ");
                }
                print!("{pid:02X} ");
            }
            println!();
        }
        Err(e) => println!("PID support probe: {e}"),
    }
    match session.poll_full() {
        Ok(vals) => {
            println!("full poll: {} signals", vals.len());
            for v in vals {
                println!(
                    "  {} = {:.2} {} (PID {:02X})",
                    v.name, v.value, v.unit, v.pid
                );
            }
        }
        Err(e) => println!("full poll: {e}"),
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
    if let Ok(ff) = session.read_freeze_frame() {
        println!("freeze frame: {}", ff.len());
        for v in ff {
            println!("  FF {} = {:.2}", v.name, v.value);
        }
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

    // One OBD PID per loop iteration, then redraw — gauges track as fast as BT allows.
    // Drain keys without blocking so tab switch never waits on a multi-PID sweep.
    let result = (|| -> Result<()> {
        loop {
            // Handle keys first (tab switch is never stuck behind full capture).
            while event::poll(Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.prev_tab(),
                        KeyCode::Char('1') => app.tab = Tab::Live,
                        KeyCode::Char('2') => app.tab = Tab::Gauges,
                        KeyCode::Char('3') => app.tab = Tab::Mfd,
                        KeyCode::Char('4') => app.tab = Tab::Dtc,
                        KeyCode::Char('5') => app.tab = Tab::Modules,
                        KeyCode::Char('6') => app.tab = Tab::Log,
                        KeyCode::Char('7') | KeyCode::Char('h') => app.tab = Tab::Help,
                        KeyCode::Char('r') => app.refresh_dtcs(),
                        KeyCode::Char('n') => app.next_gauge(),
                        KeyCode::Char('m') => app.probe_modules(),
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
                        KeyCode::Char('b') => app.cycle_bus(),
                        _ => {}
                    }
                }
            }

            if app.live_poll {
                // Always one priority PID (gauges). When capturing, one bulk PID after.
                // Never do a full multi-PID sweep in one iteration (that made gauges lag).
                app.poll_priority();
                let gauges_focus = matches!(app.tab, Tab::Gauges | Tab::Mfd);
                if app.capturing {
                    // On gauge/MFD tabs, skip bulk half the time so rpm/speed update faster.
                    let do_bulk = if gauges_focus {
                        app.bulk_flip = !app.bulk_flip;
                        app.bulk_flip
                    } else {
                        true
                    };
                    if do_bulk {
                        app.poll_bulk_capture_step();
                    }
                }
            }

            terminal.draw(|f| ui::draw(f, app))?;
        }
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

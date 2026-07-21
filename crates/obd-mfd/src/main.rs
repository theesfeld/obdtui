//! Vector MFD / pilot HUD for live OBD-II (real 2D lines, not terminal cells).

mod feed;
mod hud;

use anyhow::{Context, Result};
use clap::Parser;
use eframe::egui;
use feed::ObdFeed;
use hud::HudApp;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "obd-mfd",
    about = "Vector HUD MFD — fighter-style gauges for live OBD-II",
    version,
    long_about = "Opens a graphics window and draws real vector arcs/lines.\n\
Not a terminal TUI. Uses egui for 2D drawing.\n\
Connect: USB serial or Bluetooth SPP (same as obdtui)."
)]
struct Args {
    /// Serial port (USB). Auto-detect when omitted.
    #[arg(short, long, env = "OBDTUI_PORT")]
    port: Option<String>,

    /// Serial baud (USB/rfcomm only)
    #[arg(long)]
    baud: Option<u32>,

    /// Prefer auto|usb|bluetooth
    #[arg(long, default_value = "auto")]
    prefer: String,

    /// Bluetooth MAC for classic SPP
    #[arg(long, env = "OBDTUI_BT_MAC")]
    bt_mac: Option<String>,

    /// RFCOMM channel (default 1)
    #[arg(long, default_value_t = 1)]
    rfcomm_channel: u8,

    /// Replay a capture directory (no truck)
    #[arg(long)]
    replay: Option<PathBuf>,

    /// Window width
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Window height
    #[arg(long, default_value_t = 800)]
    height: u32,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let feed = ObdFeed::start(&args).context("start OBD feed")?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([args.width as f32, args.height as f32])
            .with_title("obd-mfd · vector HUD")
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "obd-mfd",
        options,
        Box::new(move |cc| {
            // Dark phosphor panel look
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.panel_fill = egui::Color32::from_rgb(4, 12, 8);
            style.visuals.window_fill = egui::Color32::from_rgb(4, 12, 8);
            cc.egui_ctx.set_style(style);
            Ok(Box::new(HudApp::new(feed)) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;

    Ok(())
}

// Re-export args fields for feed
impl Args {
    pub fn connect_prefer(&self) -> Result<obd_io::LinkPrefer> {
        self.prefer
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(2500)
    }
}

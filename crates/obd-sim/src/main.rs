//! Write a sample capture package for offline demos.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "obd-sim",
    about = "Write sample OBD capture packages for offline use",
    version
)]
struct Args {
    /// Output directory for the capture package
    #[arg(short, long, default_value = "fixtures/sample-session")]
    out: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut session = obd_sim::sample_session();
    session.save(&args.out)?;
    println!("Wrote sample capture to {}", args.out.display());
    Ok(())
}

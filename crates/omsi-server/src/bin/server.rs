//! Native dedicated server command. No renderer, scene, wgpu or audio.
//!
//! Full authority is deliberately disabled pending map/vehicle/network parity.
//! Explicit --experimental-free-roam runs a restricted LAN relay for testing.
use anyhow::{Context, Result, bail};
use omsi_server::{runtime, startup};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next() else {
        bail!("Usage: omsi-server <OMSI2-root> <server.cfg> [--experimental-free-roam] [--ticks N]");
    };
    let config = args.next().context("missing server.cfg path")?;
    let mut experimental = false;
    let mut max_ticks = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--experimental-free-roam" if !experimental => experimental = true,
            "--ticks" if max_ticks.is_none() => {
                let ticks: u64 = args.next().context("--ticks requires a positive integer")?.parse()?;
                anyhow::ensure!((1..=60_000).contains(&ticks), "--ticks must be 1..=60000");
                max_ticks = Some(ticks);
            }
            _ => bail!("unsupported or repeated option {flag}"),
        }
    }
    if !experimental {
        bail!("Full headless multiplayer is not ready: run the legacy openomsi --server for production. Use --experimental-free-roam only with an isolated, restricted server.cfg");
    }
    let root = PathBuf::from(root);
    anyhow::ensure!(root.is_dir(), "OMSI root must be an installed directory");
    let config = PathBuf::from(config);
    anyhow::ensure!(config.is_file(), "server.cfg must already exist");
    let plan = startup::prepare(&config, &root, None, 256, env!("CARGO_PKG_VERSION"),
        &|_, _| String::new())?;
    runtime::free_roam(&root, &plan.config, plan.weather.as_deref(), max_ticks)
}

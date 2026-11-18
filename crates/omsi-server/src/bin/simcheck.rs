//! CPU-only real-map smoke test: loads map lanes and advances production AI and people.
//! NOT an openOMSI server and NOT a valid production benchmark.
use anyhow::{Context, Result, ensure};
use omsi_server::simulation::CpuSimulation;
use omsi_server::traffic;
use omsi_server::world::HostWorld;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug)]
struct Options {
    root: PathBuf,
    map: String,
    date: i32,
    ticks: usize,
    tiles: usize,
    ai: usize,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let root = PathBuf::from(args.next().context(
        "Usage: simcheck <OMSI2-root> <map-dir-or-global.cfg> [YYYYMMDD] [ticks] [tiles] [ai]",
    )?);
    let map = args.next().context(
        "Usage: simcheck <OMSI2-root> <map-dir-or-global.cfg> [YYYYMMDD] [ticks] [tiles] [ai]",
    )?;
    let date = args.next().unwrap_or_else(|| "19890501".into()).replace('-', "");
    ensure!(
        date.len() == 8 && date.bytes().all(|c| c.is_ascii_digit()),
        "date must be YYYYMMDD"
    );
    let ticks = args.next().unwrap_or_else(|| "600".into()).parse::<usize>()?;
    let tiles = args.next().unwrap_or_else(|| "16".into()).parse::<usize>()?;
    let ai = args.next().unwrap_or_else(|| "10".into()).parse::<usize>()?;
    ensure!(args.next().is_none(), "too many arguments");
    ensure!((1..=36_000).contains(&ticks), "ticks must be 1..=36000");
    ensure!((1..=256).contains(&tiles), "tiles must be 1..=256");
    ensure!(ai <= 200, "ai must be 0..=200");
    Ok(Options {
        root,
        map,
        date: date.parse()?,
        ticks,
        tiles,
        ai,
    })
}

fn global_path(root: &Path, map: &str) -> PathBuf {
    let p = omsi_cfg::resolve_path(root, map);
    if p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cfg")) {
        p
    } else {
        omsi_cfg::resolve_path(&p, "global.cfg")
    }
}

fn run(opt: Options) -> Result<()> {
    let global = global_path(&opt.root, &opt.map);
    let mut world = HostWorld::open(&opt.root, &global, opt.date)
        .with_context(|| format!("reading CPU-only map {}", global.display()))?;
    ensure!(world.index.tiles_read > 0, "map has no readable tiles");

    // Bound the expensive map decode and terrain store; this is a smoke test,
    // not a replacement for interest-based tile streaming in a live server.
    let keys: Vec<(i32, i32)> = world
        .global
        .tiles
        .iter()
        .take(opt.tiles)
        .map(|t| (t.x, t.y))
        .collect();
    let mut network = world.network_for_tiles(&keys);
    network.left_hand = world.global.left_hand_traffic;
    let lane_count = network.lanes.len();
    let random = {
        let data = world.data.snapshot();
        traffic::random_types(
            &opt.root,
            &world.global,
            &world.map_dir,
            &data.map.ailists,
        )
    };
    let traffic = traffic::assemble(
        &opt.root,
        network,
        random,
        world.global.traffic_density_road.clone(),
        keys,
        opt.ai,
    );
    let mut sim = CpuSimulation::new(&opt.root, Arc::clone(&world.data), traffic, 0);
    sim.traffic.day_time = 9.0 * 3600.0;

    let started = Instant::now();
    let mut millis = Vec::with_capacity(opt.ticks);
    for _ in 0..opt.ticks {
        let tick = Instant::now();
        sim.step(1.0 / 60.0);
        sim.discard_presentation_events_after_publish();
        millis.push(tick.elapsed().as_secs_f64() * 1000.0);
    }
    let elapsed = started.elapsed();
    millis.sort_by(f64::total_cmp);
    let p95 = millis[((millis.len() - 1) * 95) / 100];
    let max = millis.last().copied().unwrap_or_default();
    println!(
        "CPU simcheck: map={}, indexed_tiles={}, sampled_tiles={}, lanes={}, ai_cars={}, people={}, ticks={}, simulated_seconds={:.1}, wall_seconds={:.3}, p95_tick_ms={:.3}, max_tick_ms={:.3}",
        world.global.name,
        world.index.tiles_read,
        world.loaded_tiles(),
        lane_count,
        sim.traffic.cars.len(),
        sim.people.people.len(),
        opt.ticks,
        opt.ticks as f64 / 60.0,
        elapsed.as_secs_f64(),
        p95,
        max
    );
    println!(
        "Diagnostic only: collision meshes, signal programs, scheduled AI, full passenger loading and LAN snapshots are not yet attached. These timing numbers are not production TPS."
    );
    Ok(())
}

fn main() -> Result<()> {
    run(parse(std::env::args().skip(1))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(extra: &[&str]) -> Result<Options> {
        parse(
            ["game", "Maps/Grundorf"]
                .into_iter()
                .chain(extra.iter().copied())
                .map(str::to_string),
        )
    }

    #[test]
    fn safe_defaults_keep_smoke_test_bounded() {
        let o = options(&[]).unwrap();
        assert_eq!((o.date, o.ticks, o.tiles, o.ai), (19890501, 600, 16, 10));
    }

    #[test]
    fn invalid_dates_and_resource_limits_are_rejected() {
        assert!(options(&["abc"]).is_err());
        assert!(options(&["19890501", "0"]).is_err());
        assert!(options(&["19890501", "1", "257"]).is_err());
        assert!(options(&["19890501", "1", "1", "201"]).is_err());
        assert!(options(&["19890501", "1", "1", "1", "unexpected"]).is_err());
    }

    #[test]
    fn options_preserve_custom_limits_and_map_input() {
        let o = options(&["2026-11-18", "120", "32", "15"]).unwrap();
        assert_eq!((o.date, o.ticks, o.tiles, o.ai), (20261118, 120, 32, 15));
        assert!(global_path(&o.root, &o.map).ends_with("Maps/Grundorf/global.cfg"));
        assert!(global_path(&o.root, "Maps/Grundorf/global.cfg")
            .ends_with("Maps/Grundorf/global.cfg"));
    }
}

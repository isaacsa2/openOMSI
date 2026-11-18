//! Renderer-free host runtime. It can exercise real multiplayer relay in
//! explicit *experimental free-roam* mode. Full AI, script, collisions and
//! timetable authority are blocked until CPU world parity is verified.

use crate::admin;
use crate::simulation::CpuSimulation;
use crate::tick::{FIXED_DT, TickPacer};
use crate::transport::HostTransport;
use crate::{console, traffic};
use anyhow::{Context, Result, ensure};
use glam::DVec3;
use omsi_admin::{metrics::TickMeter, profiler::Profiler};
use omsi_lan::admin::{ServerAdmin, ServerRules};
use omsi_lan::server_cfg::ServerCfg;
use omsi_net::{WorldInfo, ws::WorldCounts};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const INITIAL_TILE_LIMIT: usize = 64;

/// Measure named stages only while the operator-enabled profiler is running.
fn profile<T>(profiler: &mut Profiler, name: &'static str, f: impl FnOnce() -> T) -> T {
    let started = profiler.enabled().then(Instant::now);
    let result = f();
    if let Some(started) = started {
        profiler.stage(name, started.elapsed());
    }
    result
}

/// Bound the experimental loaded area around the map's first real entrypoint,
/// not the arbitrary ordering of [map] records in global.cfg. Without this a
/// large map could load 64 distant empty tiles and reject an otherwise valid
/// multiplayer test due to missing road paths.
fn initial_tiles(
    global: &omsi_map::global::GlobalCfg,
    index: &omsi_world::map_index::MapIndex,
    limit: usize,
) -> Vec<(i32, i32)> {
    let center = global.entry_points.iter().find_map(|entry| {
        (entry.group >= 0)
            .then(|| global.raw_tiles.get(entry.group as usize))
            .flatten()
            .copied()
            .or_else(|| index.objects.get(&entry.object_id).map(|(key, _, _)| *key))
    });
    let mut tiles: Vec<_> = global.tiles.iter().map(|t| (t.x, t.y)).collect();
    if let Some((cx, cy)) = center {
        tiles.sort_by_key(|&(x, y)| {
            let dx = (i64::from(x) - i64::from(cx)).unsigned_abs() as u128;
            let dy = (i64::from(y) - i64::from(cy)).unsigned_abs() as u128;
            dx * dx + dy * dy
        });
    }
    tiles.truncate(limit);
    tiles
}

/// Parse a configured date without inferring locale or system timezone.
pub fn checked_date(text: &str) -> Result<(i32, String)> {
    let digits = text.replace('-', "");
    ensure!(
        digits.len() == 8 && digits.bytes().all(|x| x.is_ascii_digit()),
        "date must be YYYY-MM-DD"
    );
    let year: u32 = digits[..4].parse()?;
    let month: u32 = digits[4..6].parse()?;
    let day: u32 = digits[6..].parse()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    ensure!(
        (1900..=2099).contains(&year) && day > 0 && day <= max_day,
        "invalid date"
    );
    Ok((digits.parse()?, format!("{year:04}-{month:02}-{day:02}")))
}

pub fn checked_clock(text: &str) -> Result<f64> {
    let (h, m) = text.split_once(':').context("time must be HH:MM")?;
    let h: u32 = h.parse()?;
    let m: u32 = m.parse()?;
    ensure!(h < 24 && m < 60, "time outside 00:00..23:59");
    Ok(f64::from(h * 3600 + m * 60))
}

/// Avoid pretending that a partially simulated map is a complete host.
pub fn allow_free_roam(cfg: &ServerCfg) -> Result<()> {
    ensure!(
        !cfg.passengers && !cfg.timetable && cfg.traffic == 0,
        "experimental relay requires passengers=0, timetable=0, traffic=0"
    );
    ensure!(
        !cfg.accept_uploads && !cfg.share_maps && cfg.parked_buses == 0,
        "experimental relay requires accept_uploads=0, share_maps=0, parked_buses=0"
    );
    ensure!(
        !cfg.public_server && !cfg.tunnel && cfg.public_address.eq_ignore_ascii_case("off"),
        "experimental relay requires public_server=0, tunnel=0, public_address=off"
    );
    ensure!(
        !cfg.real_time && !cfg.metar_sync,
        "experimental relay requires real_time=0 and metar_sync=0"
    );
    ensure!(
        cfg.date.is_some(),
        "experimental relay requires an explicit server.cfg date"
    );
    Ok(())
}

/// Launch a bounded CPU map and a LAN host. Explicitly not full sim parity.
pub fn free_roam(
    root: &Path,
    plan: &crate::startup::LaunchPlan,
    max_ticks: Option<u64>,
) -> Result<()> {
    let cfg = &plan.config;
    let weather = plan.weather.as_deref();
    allow_free_roam(cfg)?;
    let (date_num, date) = checked_date(cfg.date.as_deref().context("missing date")?)?;
    let initial_time = checked_clock(&cfg.time)?;
    let global = omsi_cfg::resolve_path(root, &cfg.map);
    let mut map = crate::world::HostWorld::open(root, &global, date_num)
        .with_context(|| format!("loading {}", global.display()))?;
    ensure!(map.index.tiles_read > 0, "no readable map tiles");
    let keys = initial_tiles(&map.global, &map.index, INITIAL_TILE_LIMIT);
    ensure!(!keys.is_empty(), "map has no active tiles");
    let ground_surfaces = map.install_spline_ground_surfaces(&keys);
    log::info!(
        "CPU road surfaces: {} active tiles, {} rasterized spline meshes, {} wheel meshes; {} pending scenery surfaces",
        ground_surfaces.target_tiles,
        ground_surfaces.road_meshes,
        ground_surfaces.wheel_meshes,
        ground_surfaces.pending_scenery_surfaces
    );
    let _stops = map.install_stop_lists(&keys);
    let signals = map.network_with_traffic_lights(&keys);
    let collisions = map.install_static_box_collisions(&keys, &signals.network.lanes);
    ensure!(
        !signals.network.lanes.is_empty(),
        "no drivable or foot paths in selected tiles"
    );
    // The standalone relay does not invent authoritative timetable or AI cars.
    // Free-roam has traffic=0. Loading all AI bus scripts and paints here
    // would waste disk I/O and RAM for vehicles that can never be spawned.
    let random = omsi_sim::ai_traffic::setup::RandomTypes {
        types: Vec::new(),
        groups: Vec::new(),
        group_curves: false,
        group_uvg: Vec::new(),
        uvg_defaults: Vec::new(),
    };
    let mut ai = traffic::assemble(
        root,
        signals.network,
        random,
        map.global.traffic_density_road.clone(),
        keys,
        0,
    );
    ai.lights = signals.controllers;
    ai.controller_of_object = signals.controller_of_object;
    let mut sim = CpuSimulation::new(root, Arc::clone(&map.data), ai, 0);
    sim.traffic.day_time = initial_time;
    sim.traffic.time_scale = cfg.time_speed;

    let world = WorldInfo {
        map: cfg.map.replace('\\', "/"),
        date: date.clone(),
        time: initial_time,
        weather: weather.unwrap_or_default().to_owned(),
        ..Default::default()
    };
    // Use the configured admission path: hardware bans, player limit and
    // protocol discovery must be the same as the legacy host.
    let mut net = HostTransport::bind_configured(plan, world)
        .with_context(|| format!("binding dedicated UDP port {}", cfg.port))?;
    net.session_mut().clock_speed = cfg.time_speed;
    // Do not open the gateway or allow players during CPU map initialization.
    let info = admin::info_of(cfg, env!("CARGO_PKG_VERSION"));
    omsi_lan::gateway::open_public_gateway(net.session(), info, cfg.web_port, false);
    // Do not advertise or hash an untransferable bus fleet in relay-only mode.
    log::warn!(
        "EXPERIMENTAL relay ONLY: {} static boxes, {} exact collision meshes; {} unresolved mesh files, {} tilted objects, {} attachments; AI/timetable/passengers disabled",
        collisions.static_boxes,
        collisions.static_meshes,
        collisions.pending_meshes,
        collisions.pending_tilted,
        collisions.pending_attachments
    );
    eprintln!(
        "Experimental headless FREE-ROAM relay: no authoritative bus/traffic/passenger simulation parity"
    );
    net.ready();
    let port = net
        .session()
        .local_addr()
        .map_or(cfg.port, |addr| addr.port());
    log::info!("native headless test host ready; UDP port {port}");
    if let Some(code) = net.session().code() {
        log::info!("join code for approved testers: {}", code.encode());
    }
    log::info!(
        "web gateway on TCP port {} (public_address=off does not restrict firewall access)",
        cfg.web_port
    );
    let mut meter = TickMeter::new(60.0);
    let mut profiler = Profiler::new();
    let mut admin_state = ServerAdmin::default();
    admin_state.password = cfg.admin_password.clone();
    admin_state.moderator_password = cfg.moderator_password.clone();
    let rules = ServerRules {
        real_time: false,
        metar: false,
        traffic_max: 0,
        notice_ok: &|notice| notice.len() <= 2048,
    };
    let mut pacer = TickPacer::new(Instant::now());
    let mut world_sync = crate::lan_world::HeadlessWorldSync::default();
    let mut clock = initial_time;
    let mut n = 0_u64;
    let mut report = Instant::now();
    loop {
        if console::stop_requested() || max_ticks.is_some_and(|limit| n >= limit) {
            break;
        }
        let started = Instant::now();
        let positions: HashMap<u32, (DVec3, f64)> = net
            .session()
            .peers()
            .filter(|peer| {
                peer.has_pose
                    && peer.has_info
                    && peer.pose.x.is_finite()
                    && peer.pose.y.is_finite()
                    && peer.pose.z.is_finite()
                    && peer.pose.heading.is_finite()
            })
            .map(|peer| {
                (
                    peer.pose.id,
                    (
                        DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z),
                        f64::from(peer.pose.heading),
                    ),
                )
            })
            .collect();
        let hub = omsi_lan::admin::hub().map(|h| &**h);
        crate::commands::service_commands(
            net.session_mut(),
            &mut admin_state,
            &meter,
            &mut profiler,
            &|id| positions.get(&id).copied(),
            &rules,
            hub,
            &|_, _, _| None,
            FIXED_DT,
        );
        crate::commands::apply_world_jobs(net.session_mut(), &mut admin_state, None, clock, 0, hub);
        clock += f64::from(FIXED_DT) * net.session().clock_speed;
        let authoritative = clock + admin_state.shift;
        net.session_mut().set_clock(&date, authoritative);
        profile(&mut profiler, "network", || net.tick(FIXED_DT));
        profile(&mut profiler, "interest", || {
            sim.sync_host_clients(net.session_mut())
        });
        // Match LAN clock shifts; TrafficSim advances day_time inside step.
        sim.traffic.day_time = authoritative - f64::from(FIXED_DT) * sim.traffic.time_scale;
        profile(&mut profiler, "simulation", || sim.step(FIXED_DT));
        profile(&mut profiler, "world_sync", || {
            world_sync.tick(net.session_mut(), &sim.traffic, FIXED_DT)
        });
        // AI vehicle and passenger snapshots remain unsupported in free-roam.
        sim.discard_presentation_events_after_publish();
        profiler.tick(started.elapsed());
        meter.completed(Instant::now());
        n += 1;
        if report.elapsed() >= Duration::from_secs(1) {
            let counts = WorldCounts {
                traffic: 0,
                passengers: false,
                ..Default::default()
            };
            admin::publish_status(
                net.session_mut(),
                &mut admin_state,
                authoritative,
                weather.unwrap_or_default(),
                None,
                counts,
                &meter,
                &profiler,
            );
            report = Instant::now();
        }
        std::thread::sleep(pacer.after_tick(Instant::now()).wait);
    }
    net.session_mut().leave();
    omsi_lan::gateway::close_public_gateway();
    eprintln!("Dedicated relay stopped cleanly after {n} ticks");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiling_is_off_by_default_and_records_cpu_stages_only_after_opt_in() {
        let mut profiler = Profiler::new();
        assert_eq!(profile(&mut profiler, "network", || 7_u32), 7);
        assert!(profiler.report().stages.is_empty());
        profiler.start();
        assert_eq!(profile(&mut profiler, "simulation", || 9_u32), 9);
        assert_eq!(profiler.report().stages[0].name, "simulation");
        profiler.stop();
        profile(&mut profiler, "network", || ());
        assert_eq!(profiler.report().stages.len(), 1);
    }

    #[test]
    fn experimental_initial_tiles_start_near_the_actual_entrypoint() {
        use omsi_map::global::{EntryPoint, MapTileRef};
        let mut global = omsi_map::global::GlobalCfg::default();
        global.tiles = (0..100)
            .map(|i| MapTileRef {
                x: i,
                y: 0,
                index: i as usize,
                ..Default::default()
            })
            .collect();
        global.raw_tiles = (0..100).map(|x| (x, 0)).collect();
        global.entry_points.push(EntryPoint {
            group: 80,
            object_id: 95,
            ..Default::default()
        });
        let index = omsi_world::map_index::MapIndex::default();
        let selected = initial_tiles(&global, &index, 5);
        assert_eq!(selected.len(), 5);
        assert_eq!(selected[0], (80, 0));
        assert!(selected.iter().all(|&(x, y)| (x - 80).abs() <= 3 && y == 0));
    }

    #[test]
    fn tiles_keep_map_order_when_no_entrypoint_exists() {
        let mut global = omsi_map::global::GlobalCfg::default();
        global.tiles = vec![
            omsi_map::global::MapTileRef {
                x: 10,
                ..Default::default()
            },
            omsi_map::global::MapTileRef {
                x: 20,
                ..Default::default()
            },
        ];
        assert_eq!(
            initial_tiles(&global, &omsi_world::map_index::MapIndex::default(), 1),
            vec![(10, 0)]
        );
    }

    #[test]
    fn invalid_time_and_date_cannot_start_a_session() {
        for t in ["24:00", "08:60", "8:xx", "nan", "01:02:03"] {
            assert!(checked_clock(t).is_err());
        }
        assert_eq!(checked_clock("08:30").unwrap(), 30_600.0);
        assert_eq!(
            checked_date("1989-05-30").unwrap(),
            (19890530, "1989-05-30".into())
        );
        assert!(checked_date("2050-99-01").is_err());
        assert!(checked_date("2026-02-29").is_err());
        assert!(checked_date("2024-02-29").is_ok());
        assert!(checked_date("2024-04-31").is_err());
    }
    #[test]
    fn experimental_admission_fails_closed_on_public_or_transfer_settings() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("omsi-headless-{}-{nanos}.cfg", std::process::id()));
        // Package defaults must pass the same admission guard as production
        // runtime parsing, preventing accidental unsafe test ZIPs.
        let config = include_str!("../../../scripts/server-headless/server-test.cfg");
        std::fs::write(&path, config).unwrap();
        let mut cfg = ServerCfg::load(&path, 256).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(allow_free_roam(&cfg).is_ok());
        cfg.accept_uploads = true;
        assert!(allow_free_roam(&cfg).is_err());
        cfg.accept_uploads = false;
        cfg.share_maps = true;
        assert!(allow_free_roam(&cfg).is_err());
        cfg.share_maps = false;
        cfg.public_address.clear();
        assert!(allow_free_roam(&cfg).is_err());
        cfg.public_address = "off".into();
        cfg.tunnel = true;
        assert!(allow_free_roam(&cfg).is_err());
    }
}

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

/// Parse a configured date without inferring locale or system timezone.
pub fn checked_date(text: &str) -> Result<(i32, String)> {
    let digits = text.replace('-', "");
    ensure!(digits.len() == 8 && digits.bytes().all(|x| x.is_ascii_digit()), "date must be YYYY-MM-DD");
    let year: u32 = digits[..4].parse()?;
    let month: u32 = digits[4..6].parse()?;
    let day: u32 = digits[6..].parse()?;
    ensure!((1900..=2099).contains(&year) && (1..=12).contains(&month)
        && (1..=31).contains(&day), "invalid date");
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
    ensure!(!cfg.passengers && !cfg.timetable && cfg.traffic == 0,
        "experimental relay requires passengers=0, timetable=0, traffic=0");
    ensure!(!cfg.accept_uploads && !cfg.share_maps && cfg.parked_buses == 0,
        "experimental relay requires accept_uploads=0, share_maps=0, parked_buses=0");
    ensure!(!cfg.public_server && !cfg.tunnel
        && cfg.public_address.eq_ignore_ascii_case("off"),
        "experimental relay requires public_server=0, tunnel=0, public_address=off");
    ensure!(!cfg.real_time && !cfg.metar_sync,
        "experimental relay requires real_time=0 and metar_sync=0");
    ensure!(cfg.date.is_some(), "experimental relay requires an explicit server.cfg date");
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
    let keys: Vec<(i32, i32)> = map.global.tiles.iter()
        .take(INITIAL_TILE_LIMIT).map(|t| (t.x, t.y)).collect();
    ensure!(!keys.is_empty(), "map has no active tiles");
    let _stops = map.install_stop_lists(&keys);
    let signals = map.network_with_traffic_lights(&keys);
    let collisions = map.install_static_box_collisions(&keys, &signals.network.lanes);
    ensure!(!signals.network.lanes.is_empty(), "no drivable or foot paths in selected tiles");
    // The standalone relay does not invent authoritative timetable or AI cars.
    // Free-roam has traffic=0. Loading all AI bus scripts and paints here
    // would waste disk I/O and RAM for vehicles that can never be spawned.
    let random = omsi_sim::ai_traffic::setup::RandomTypes {
        types: Vec::new(), groups: Vec::new(), group_curves: false,
        group_uvg: Vec::new(), uvg_defaults: Vec::new(),
    };
    let mut ai = traffic::assemble(root, signals.network, random,
        map.global.traffic_density_road.clone(), keys, 0);
    ai.lights = signals.controllers;
    ai.controller_of_object = signals.controller_of_object;
    let mut sim = CpuSimulation::new(root, Arc::clone(&map.data), ai, 0);

    let world = WorldInfo {
        map: cfg.map.replace('\\', "/"), date: date.clone(),
        time: initial_time, weather: weather.unwrap_or_default().to_owned(),
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
        "EXPERIMENTAL relay ONLY: {} static boxes, {} unresolved collision meshes, {} attached objects; AI/timetable/passengers disabled",
        collisions.static_boxes, collisions.pending_meshes, collisions.pending_attachments
    );
    eprintln!("Experimental headless FREE-ROAM relay: no authoritative bus/traffic/passenger simulation parity");
    net.ready();
    let mut meter = TickMeter::new(60.0);
    let mut profiler = Profiler::new();
    let mut admin_state = ServerAdmin::default();
    admin_state.password = cfg.admin_password.clone();
    admin_state.moderator_password = cfg.moderator_password.clone();
    let rules = ServerRules {
        real_time: false, metar: false, traffic_max: 0,
        notice_ok: &|notice| notice.len() <= 2048,
    };
    let mut pacer = TickPacer::new(Instant::now());
    let mut clock = initial_time;
    let mut n = 0_u64;
    let mut report = Instant::now();
    loop {
        if console::stop_requested() || max_ticks.is_some_and(|limit| n >= limit) {
            break;
        }
        let started = Instant::now();
        let positions: HashMap<u32, (DVec3, f64)> = net.session().peers()
            .filter(|peer| peer.has_pose && peer.has_info
                && peer.pose.x.is_finite() && peer.pose.y.is_finite()
                && peer.pose.z.is_finite() && peer.pose.heading.is_finite())
            .map(|peer| (peer.pose.id,
                (DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z), f64::from(peer.pose.heading))))
            .collect();
        let hub = omsi_lan::admin::hub().map(|h| &**h);
        crate::commands::service_commands(net.session_mut(), &mut admin_state,
            &meter, &mut profiler, &|id| positions.get(&id).copied(),
            &rules, hub, &|_, _, _| None, FIXED_DT);
        crate::commands::apply_world_jobs(net.session_mut(), &mut admin_state,
            None, clock, 0, hub);
        clock += f64::from(FIXED_DT) * net.session().clock_speed;
        let authoritative = clock + admin_state.shift;
        net.session_mut().set_clock(&date, authoritative);
        net.tick(FIXED_DT);
        sim.sync_host_clients(net.session_mut());
        sim.step(FIXED_DT);
        // No custom snapshots are published in this experimental mode.
        sim.discard_presentation_events_after_publish();
        profiler.tick(started.elapsed());
        meter.completed(Instant::now());
        n += 1;
        if report.elapsed() >= Duration::from_secs(1) {
            let counts = WorldCounts {
                traffic: 0, passengers: false, ..Default::default()
            };
            admin::publish_status(net.session_mut(), &mut admin_state,
                authoritative, weather.unwrap_or_default(), None,
                counts, &meter, &profiler);
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
    fn invalid_time_and_date_cannot_start_a_session() {
        for t in ["24:00", "08:60", "8:xx", "nan", "01:02:03"] {
            assert!(checked_clock(t).is_err());
        }
        assert_eq!(checked_clock("08:30").unwrap(), 30_600.0);
        assert_eq!(checked_date("1989-05-30").unwrap(), (19890530, "1989-05-30".into()));
        assert!(checked_date("2050-99-01").is_err());
    }
}

//! The dedicated server (`omsi --server server.cfg`): a session host that is always on, like
//! a Minecraft server - no window, no sound, no graphics card (the renderer runs on wgpu's
//! no-op device: the world, the traffic, the timetable and the people are simulated as the
//! host's game simulates them, and nothing is drawn). Players reach it at `host:port` (UDP)
//! or at its web address over a WebSocket (`omsi_net::ws`), which is what a free Cloudflare
//! tunnel carries: `tunnel = 1` starts one and prints the `https://….trycloudflare.com`
//! address to add in the launcher's Multiplayer → Servers.
//!
//! The folder of `server.cfg` is the server's: `server-icon.png` beside it is its icon (a
//! 64x64 PNG, like Minecraft's), `server.log` its log. A missing `server.cfg` is written with
//! the defaults and a comment for every key.

use super::*;
use std::path::Path;

pub(crate) use omsi_server::console;

pub(crate) use omsi_lan::server_cfg::{SERVER_VEHICLES, ServerCfg, enforce_vehicles, player_info};

pub(crate) use omsi_server::fleet::announce_fleet;

/// The server's status as the web gateway tells it (see `omsi_net::ws::ServerInfo`).
pub(crate) fn info_of(cfg: &ServerCfg, _root: &Path) -> omsi_net::ws::ServerInfo {
    omsi_net::ws::ServerInfo {
        name: cfg.name.clone(),
        motd: cfg.motd.clone(),
        map: cfg.map.clone(),
        players: 0,
        max_players: cfg.max_players,
        version: format!("{} ({})", omsi_platform::VERSION, BUILD),
        icon: cfg.icon.clone(),
        time: cfg.time.clone(),
        weather: cfg.weather.clone().unwrap_or_default(),
        password: false,
        vehicles: cfg.vehicles.clone(),
        // (announced once checked: `announce_fleet`)
        fleet: Vec::new(),
        fleet_version: 0,
        reached_at: String::new(),
        players_public: cfg.share_positions,
        player_list: Vec::new(),
        local_admin_password: cfg.admin_password.clone(),
        direct: Some(cfg.public_address.clone())
            .filter(|a| a != "off")
            .unwrap_or_default(),
        ..Default::default()
    }
}

/// The floor nearest the height a parked bus stands at (a bus on the lower of two floors
/// stays on it); only where there is none, the one further up, then the terrain.
pub(crate) fn parked_floor(world: &World, x: f64, y: f64, near: f64) -> Option<f64> {
    world
        .stand_height(x, y, near)
        .or_else(|| world.stand_height(x, y, near + 4.0))
        .or_else(|| world.ground_height(x, y))
}

pub(crate) fn park_buses(server: bool, args: &Args, world: &World) {
    if server {
        let spots: Vec<omsi_lan::parked::Spot> = world
            .global
            .entry_points
            .iter()
            .filter_map(|e| world.entry_point_place(e))
            .map(|(at, h)| (at.to_array(), h[0] as f32))
            .collect();
        omsi_lan::parked::start(&args.root, &args.map, &spots, &|x, y, near| {
            parked_floor(world, x, y, near)
        });
    }
}

/// Set up the arguments for a server run from `server.cfg` (the rest is the offscreen
/// host loop, see `offscreen::run_offscreen`).
pub(crate) fn prepare(args: &mut Args, path: &Path) -> Result<ServerCfg> {
    let mut cfg = ServerCfg::load(path, crate::game_lists::TRAFFIC_MAX)?;
    // (the map a start script picked, over server.cfg's)
    if let Some(m) = args.server_map.take().filter(|m| !m.trim().is_empty()) {
        cfg.map = m.trim().replace('\\', "/");
    }
    SERVER_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = SERVER_ADMIN.set((
        cfg.admin_password.clone(),
        cfg.moderator_password.clone(),
        if cfg.real_time { 1.0 } else { cfg.time_speed },
    ));
    omsi_server::admin::start_administration(
        &cfg,
        path,
        crate::game_lists::TRAFFIC_MAX,
        &format!("{} ({})", omsi_platform::VERSION, BUILD),
    );
    console::start();
    omsi_lan::server_cfg::set_server_speed_limit_kmh(cfg.speed_limit_kmh);
    log::info!(
        "server: RP speed limit: {}",
        if cfg.speed_limit_kmh == 0 {
            "disabled".to_string()
        } else {
            format!("{} km/h", cfg.speed_limit_kmh)
        }
    );
    omsi_lan::admin::allow_player_teleport(cfg.allow_player_teleport);
    // the machine bans: this server's list beside server.cfg, and the owner's (all my sessions)
    omsi_lan::bans::announce_local_hardware_id();
    omsi_lan::bans::require_hardware_id(cfg.require_hardware_id);
    omsi_lan::bans::install(omsi_lan::bans::BanStore::open(
        Some(cfg.dir.join("bans.json")),
        cfg.owner_bans
            .clone()
            .or_else(omsi_lan::bans::BanStore::default_owner_path),
    ));
    crate::real_time::set_server_real(cfg.real_time);
    let _ = SERVER_VEHICLES.set(cfg.vehicles.clone());
    cfg.apply_to_mods();
    crate::logmask::keep_whole(cfg.log_addresses);
    if cfg.log_addresses {
        log::info!("server: log_addresses = 1: the players' internet addresses are logged whole");
    }
    args.map = cfg.map.clone();
    args.time = cfg.time.clone();
    if let Some(d) = &cfg.date {
        args.date = Some(d.clone());
    }
    if cfg.real_time {
        crate::real_time::start_at_now(args);
    }
    args.weather = cfg.weather.clone();
    // the METAR sync: the report's weather from the start (the host loop downloads it again)
    let station = cfg.metar_sync.then(|| {
        if cfg.metar_station.is_empty() {
            omsi_launcher_lib::nearest_airport(&args.root.to_string_lossy(), &args.map)
        } else {
            cfg.metar_station.clone()
        }
    });
    if let Some(icao) = station.as_ref() {
        match crate::weather_setup::try_metar(icao)
            .and_then(|w| crate::weather_setup::report_wire(&w))
        {
            Some(wire) => args.weather = Some(wire),
            None => log::warn!("server: no METAR report for {icao} yet; trying again soon"),
        }
        log::info!("server: the weather follows the METAR report of {icao}");
    }
    let _ = SERVER_METAR.set(station);
    args.traffic = cfg.traffic;
    args.schedule = cfg.timetable;
    args.ai_drive_through_parked = cfg.ai_drive_through_parked;
    if cfg.ai_drive_through_parked {
        log::info!(
            "server: ai_drive_through_parked = 1: the AI drives on through a parked car standing in its lane, as OMSI 2's does"
        );
    }
    args.passengers = cfg.passengers;
    let _ = SERVER_PASSENGERS.set((cfg.passengers, cfg.passenger_density));
    if cfg.public_address.is_empty() {
        omsi_net::addrs::auto_direct(cfg.web_port);
    }
    let limits = cfg.ai_limits.settings_lines();
    if !limits.is_empty() {
        log::info!(
            "server: AI limits of server.cfg over settings.cfg: {}",
            limits.trim_end().replace('\n', ", ")
        );
        let _ = crate::settings::SERVER_AI_LIMITS.set(limits);
    }
    args.lan_host = Some(cfg.port);
    args.lan_join = None;
    args.lan_name = cfg.name.clone();
    args.bus = None;
    args.radius = Some(if cfg.radius <= 0 { 99 } else { cfg.radius });
    args.size = "64x36".into();
    if args.offscreen.is_none() {
        args.offscreen = Some(std::env::temp_dir().join("omsi-server-unused.png"));
    }
    let density = cfg
        .passenger_density
        .map(|d| d.to_string())
        .unwrap_or_else(|| "as settings.cfg".into());
    log::info!(
        "server: you run this server, not openOMSI: you are responsible for the data your players send it (driver names, network addresses, chat, voice) and for what you keep of it, such as logs and bans"
    );
    log::info!(
        "server '{}': map {}, {} at {}, traffic {}, timetable {}, passengers {} (density {density}), UDP {} / web {}, at most {} players",
        cfg.name,
        cfg.map,
        cfg.date.as_deref().unwrap_or("today"),
        cfg.time,
        cfg.traffic,
        cfg.timetable,
        cfg.passengers,
        cfg.port,
        cfg.web_port,
        cfg.max_players
    );
    Ok(cfg)
}

/// A dedicated server's people: whether it has any (`passengers`) and its
/// `passenger_density` (None: the machine's own passengers setting).
pub(crate) static SERVER_PASSENGERS: std::sync::OnceLock<(bool, Option<f32>)> =
    std::sync::OnceLock::new();

/// The map of `server.cfg` is installed (in the original folder or the content folder), or
/// what to do: the maps there are.
pub(crate) fn check_map(args: &Args) -> Result<(), String> {
    let path = omsi_cfg::resolve_path(&args.root, &args.map);
    if omsi_cfg::vfs::exists(&path) {
        return Ok(());
    }
    Err(format!(
        "the map {} (server.cfg's map, or --server-map) is not installed ({} not found). Installed maps: {}",
        args.map,
        path.display(),
        Some(installed_maps().join(", "))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "none".into())
    ))
}

pub(crate) use omsi_lan::server_cfg::installed_maps;

/// The airport whose METAR report a dedicated server's weather follows (None: no METAR sync).
pub(crate) static SERVER_METAR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// A dedicated server's admin and moderator passwords and clock speed (for the host loop).
pub(crate) static SERVER_ADMIN: std::sync::OnceLock<(String, String, f64)> =
    std::sync::OnceLock::new();

pub(crate) use omsi_server::admin::{admin_state, tick_status};

/// A dedicated server run: graphics without a device, the whole world by interest.
pub(crate) static SERVER_MODE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Start publishing the dedicated host in the public server list.
pub(crate) fn start_public_list(cfg: &ServerCfg) {
    omsi_server::admin::start_public_list(cfg, &format!("{} ({})", omsi_platform::VERSION, BUILD));
}

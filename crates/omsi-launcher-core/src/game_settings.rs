use super::*;

// ---------------------------------------------------------------------------------------
// settings (the game's ~/.openomsi/settings.cfg)

pub fn get_settings() -> Result<Value> {
    let text = std::fs::read_to_string(data_dir().join("settings.cfg")).ok();
    let mut v = settings_from_text(text.as_deref());
    // (what the page may offer beside the upscalers that are always there; not a setting)
    let game_dir =
        find_game(&load_config_raw().game).and_then(|g| g.parent().map(Path::to_path_buf));
    v["temporal_upscalers"] = json!(
        game_dir.map_or_else(omsi_cfg::upscaler_files::of_the_system, |d| {
            omsi_cfg::upscaler_files::available(&d)
        })
    );
    v["frame_generators"] = json!(omsi_cfg::upscaler_files::frame_generation_choices());
    // no settings of our own yet: start from what the player set in OMSI 2
    if text.is_none() {
        if let Some(o) = root().ok().and_then(|r| omsi_options(&r)) {
            if let (Some(dst), Some(src)) = (v.as_object_mut(), o.settings.as_object()) {
                for (k, x) in src {
                    dst.insert(k.clone(), x.clone());
                }
            }
            v["imported_from_omsi"] = json!(true);
        }
    }
    Ok(v)
}

/// Other spellings the game reads for a key the launcher writes: they are read here too and
/// dropped when the file is written again (the game takes the last line of them, so a kept
/// `lang=` would undo the page's `language=`).
// (`navigator_opacity`: the opacity was the navigator's before it was the whole interface's)
const SETTING_ALIASES: &[(&str, &str)] = &[
    ("af", "anisotropy"),
    ("ambient_occlusion", "ssao"),
    ("fractal", "detail_textures"),
    ("lang", "language"),
    ("texmemlimit", "texture_memory"),
    ("navigator_opacity", "ui_opacity"),
    ("gear_buttons_hold", "momentary_gears"),
];

/// Keys of before that are dropped when the file is written again: `texture_memory=` (and
/// OMSI's `texmemlimit=`, its alias) was the textures' budget, which `video_memory=` - all
/// the game holds on the card - replaced; kept, a 401 or a 1000 meant for the textures would
/// be taken for something else by whoever reads it next.
const RETIRED_SETTINGS: &[&str] = &["texture_memory", "nav_light"];

/// A window size as the settings keep it: "WxH" in pixels (each 320..16384), else "auto".
pub fn resolution_text(v: &str) -> String {
    let v = v
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "")
        .replace(['*', '×'], "x");
    match v
        .split_once('x')
        .map(|(w, h)| (w.trim().parse::<u32>(), h.trim().parse::<u32>()))
    {
        Some((Ok(w), Ok(h))) if (320..=16384).contains(&w) && (240..=16384).contains(&h) => {
            format!("{w}x{h}")
        }
        _ => "auto".into(),
    }
}

fn setting_key(k: &str) -> String {
    let k = k.trim().to_ascii_lowercase();
    SETTING_ALIASES
        .iter()
        .find(|(alias, _)| *alias == k)
        .map(|(_, key)| key.to_string())
        .unwrap_or(k)
}

// (the language table lives in omsi-cfg, below the input crate that needs it too)
pub use omsi_cfg::lang::{LANGUAGES, language_code, language_iso};

/// `vanilla` (as OMSI 2), `vanilla_plus`, `enhanced` or `enhanced_plus`, from the ways a file
/// may spell them (as the game's `settings::graphics_mode`).
pub fn graphics_mode(v: &str) -> &'static str {
    match v
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
        .as_str()
    {
        "enhanced_plus" | "enhanced+" | "enhancedplus" | "enhanced_+" | "2" => "enhanced_plus",
        "enhanced" | "1" => "enhanced",
        "vanilla" | "classic" | "original" | "omsi" | "omsi2" | "omsi_2" => "vanilla",
        _ => "vanilla_plus",
    }
}

/// The graphics mode a settings page's `v` draws with, ray tracing counted as a mode of its own:
/// `vanilla`, `vanilla_plus`, `enhanced`, or `enhanced_plus` for Enhanced with Real-Time Ray
/// Tracing on (what the code that tunes for the traced picture asks).
pub fn effective_mode(v: &Value) -> &'static str {
    let g = graphics_mode(
        v.get("graphics")
            .and_then(|x| x.as_str())
            .unwrap_or("vanilla_plus"),
    );
    if g == "enhanced"
        && v.get("ray_tracing")
            .and_then(|x| x.as_bool())
            .unwrap_or(false)
    {
        "enhanced_plus"
    } else {
        g
    }
}

/// The Graphics settings' quality presets: Low, Medium, High, Extra, Max and Ultra, shared by the
/// launcher and the game's options. A preset sets the graphics mode as well - Low is Vanilla (as
/// OMSI 2 draws), Medium Vanilla+ (the defaults), High and above Enhanced - and, in Enhanced, how
/// much of its light is switched on: High enables the street lamps' shadows, Extra the clouds' shadow, the
/// glow of lights, the street light and the reflections in full, Max every light's shadows and the
/// mirrors drawn as the picture, Ultra Real-Time Ray Tracing on top of all of it. Each is a step up in cost and the
/// light's own shadows (cached tiles, a handful of passes) are far from the dearest of them. The
/// texture memory is left as it is.
pub fn graphics_presets() -> [(&'static str, Value); 6] {
    // per level: Low, Medium, High, Extra, Max, Ultra
    let graphics = [
        "vanilla",
        "vanilla_plus",
        "enhanced",
        "enhanced",
        "enhanced",
        "enhanced",
    ];
    let msaa = [1, 4, 2, 4, 4, 4];
    let scale = ["0.75", "auto", "auto", "auto", "auto", "auto"];
    let anisotropy = [2, 8, 8, 8, 16, 16];
    let shadow = [1024, 2048, 2048, 2048, 4096, 4096];
    let mirror = [128, 256, 256, 512, 512, 512];
    let refresh = ["eco", "full", "full", "full", "full", "full"];
    let view = ["600", "auto", "auto", "auto", "2000", "2000"];
    let obj_size = [0.03, 0.013, 0.013, 0.01, 0.005, 0.005];
    let obj_dist = ["500", "auto", "auto", "auto", "1500", "1500"];
    let rain = ["low", "high", "high", "high", "high", "high"];
    let light_shadows = ["off", "off", "street", "street", "all", "all"];
    let street_light = ["off", "off", "off", "auto", "on", "on"];
    let puddles = ["off", "high", "low", "high", "high", "high"];
    let mirror_quality = [
        "standard", "standard", "standard", "standard", "enhanced", "enhanced",
    ];
    std::array::from_fn(|i| {
        let low = i == 0;
        let preset = json!({
            "graphics": graphics[i], "ray_tracing": i == 5, "msaa": msaa[i], "anisotropy": anisotropy[i],
            "shadow_size": shadow[i], "clouds": true, "rain_quality": rain[i], "windy_trees": !low, "detailed_trees": !low,
            "view_distance": view[i], "min_obj_size": obj_size[i], "max_obj_dist": obj_dist[i],
            "mirror_size": mirror[i], "mirror_refresh": refresh[i], "mirror_quality": mirror_quality[i], "render_scale": scale[i],
            "detail_textures": !low, "ssao": !low, "shadows": !low, "reflections": 1.0, "road_glare": 1.0, "traffic_headlights": 1.0,
            "light_shadows": light_shadows[i], "cloud_shadows": i >= 3, "light_glow": i >= 3,
            "street_light": street_light[i], "puddles": puddles[i],
        });
        let mut preset = preset;
        // (Low is Vanilla, which draws none of these: it leaves them as they are, so that going back
        // to Vanilla+ does not find them off)
        if low {
            for k in ["ssao", "shadows", "detail_textures"] {
                preset.as_object_mut().map(|o| o.remove(k));
            }
        }
        (
            ["Low", "Medium", "High", "Extra", "Max", "Ultra"][i],
            preset,
        )
    })
}

/// Normalize vehicle smoke visibility for the game, launcher and saved profiles.
pub fn vehicle_smoke(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "player" | "player_only" => "player",
        "off" | "none" => "off",
        _ => "all",
    }
}

/// The wet roads' reflections: `off` or `low`, else `high` (as the game reads its `puddles`).
pub fn puddles(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "none" => "off",
        "low" => "low",
        _ => "high",
    }
}

/// The enhanced clouds' quality: `low`, else `high` (as the game's `settings::cloud_quality`).
fn cloud_quality(x: &str) -> &'static str {
    if x.trim().eq_ignore_ascii_case("low") {
        "low"
    } else {
        "high"
    }
}

/// Which lights cast shadows: `off` or `all`, else `street` (as the game's `settings::light_shadows`).
fn light_shadows(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "none" | "false" => "off",
        "all" | "2" | "every" | "max" => "all",
        _ => "street",
    }
}

/// The street-scale light: `on` or `off`, else `auto` (as the game's `settings::street_light`).
fn street_light(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => "on",
        "off" | "0" | "false" => "off",
        _ => "auto",
    }
}

/// The rain's quality: `medium` or `low`, else `high` (as the game's `settings::rain_quality`).
fn rain_quality(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "medium" => "medium",
        "low" => "low",
        _ => "high",
    }
}

/// How often the mirrors are drawn: `off`, `eco` or `full`, also from OMSI's
/// `performance_realreflexions` (none, economy, full).
pub(super) fn mirror_refresh(x: &str) -> &'static str {
    match x.trim().to_ascii_lowercase().as_str() {
        "off" | "none" => "off",
        "eco" | "economy" => "eco",
        "every" | "every_frame" => "every",
        _ => "full",
    }
}

/// How the mirrors are shaded in the Enhanced pictures: `standard` (the plain light) or
/// `enhanced` (as the window's picture).
fn mirror_quality(x: &str) -> &'static str {
    if x.trim().eq_ignore_ascii_case("enhanced") {
        "enhanced"
    } else {
        "standard"
    }
}

pub fn perf_overlay_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "compact" | "1" | "true" | "on" => "compact",
        "detailed" | "2" => "detailed",
        _ => "off",
    }
}

/// Where the head's pose comes from (`head_tracking_source`): "opentrack", "tobii" (a Tobii
/// eye tracker read natively) or "auto" (both, opentrack's first).
pub fn head_tracking_source(v: &str) -> &'static str {
    match v.trim().to_ascii_lowercase().as_str() {
        "opentrack" | "udp" => "opentrack",
        "tobii" => "tobii",
        _ => "auto",
    }
}

/// A Tobii tracker's smoothing (`head_tracking_smoothing`, 0 to 1, 0.5) and dead zone
/// (`head_tracking_dead_zone`, 0 to 1 degrees, 0.1), in twentieths (none, or one out of its
/// reach: the default).
pub fn head_tracking_value(key: &str, v: Option<f64>) -> f64 {
    let default = if key == "head_tracking_smoothing" {
        0.5
    } else {
        0.1
    };
    v.filter(|x| x.is_finite() && (0.0..=1.0).contains(x))
        .map(|x| (x * 20.0).round() / 20.0)
        .unwrap_or(default)
}

fn ui_theme_from_text(text: &str) -> &'static str {
    let (mut theme, mut legacy_nav_light) = (None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (setting_key(key), value.trim());
        match key.as_str() {
            "ui_theme" => {
                theme = Some(if value.eq_ignore_ascii_case("light") {
                    "light"
                } else {
                    "dark"
                })
            }
            "nav_light" => {
                legacy_nav_light = Some(matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "on" | "yes"
                ))
            }
            _ => {}
        }
    }
    theme.unwrap_or(if legacy_nav_light == Some(true) {
        "light"
    } else {
        "dark"
    })
}

/// Dynamic and map-placed arrows, each falling back to the former shared setting.
pub fn route_arrows_from_text(text: &str) -> (bool, bool) {
    let mut legacy = false;
    let mut dynamic = None;
    let mut map = None;
    for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
        let on = || {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "on" | "yes"
            )
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "nav_arrows" => legacy = on(),
            "nav_dynamic_arrows" => dynamic = Some(on()),
            "nav_map_arrows" => map = Some(on()),
            _ => {}
        }
    }
    (dynamic.unwrap_or(legacy), map.unwrap_or(legacy))
}

/// The page's view of a `settings.cfg` text (None: no file yet, the game's defaults).
pub fn settings_from_text(text: Option<&str>) -> Value {
    settings_from_text_on(text, cfg!(any(target_os = "android", target_os = "ios")))
}

// Voice chat in multiplayer: off until switched on (the microphone is then used with a
// talk key held, or open).
fn voice_defaults(v: &mut Value) {
    v["voice_enabled"] = json!(false);
    v["voice_mode"] = json!("ptt");
    v["voice_mic_gain"] = json!(1.0);
    v["voice_threshold_db"] = json!(-45);
    v["voice_input_device"] = json!("");
    v["voice_volume"] = json!(1.0);
    v["voice_volume_nearby"] = json!(1.0);
    v["voice_volume_radio"] = json!(1.0);
    v["voice_range"] = json!(75);
    v["voice_talker_list"] = json!(true);
    v["voice_all_off"] = json!(false);
}

// Keep independent opt-in defaults shared by desktop and phone settings.
fn passenger_defaults(v: &mut Value) {
    v["passenger_connections"] = json!(false);
    v["passenger_transfer_percent"] = json!(20.0);
    v["passenger_hailing"] = json!(false);
    v["passenger_full_bus_reaction"] = json!(false);
    v["pax_short_rides_stand"] = json!(false);
}

/// Initial values shared by desktop and phone, kept outside the long settings parser.
fn graphics_and_interface_defaults(v: &mut Value) {
    v["upscaler"] = json!("default");
    v["upscale_quality"] = json!("quality");
    v["frame_generation"] = json!("off");
    v["fsr_quality"] = json!("quality");
    v["fsr_sharpness"] = json!(0.8);
    v["street_light"] = json!("off");
    v["ray_tracing"] = json!(false);
    v["light_shadows"] = json!("off");
    v["cloud_shadows"] = json!(false);
    v["light_glow"] = json!(false);
    v["pause_quick_actions"] = json!("default");
    // (the interface theme's id: "openomsi" is openOMSI's own look)
    v["ui_theme_pack"] = json!("openomsi");
    v["ui_theme"] = json!("dark");
    v["road_glare"] = json!(1.0);
    v["traffic_headlights"] = json!(1.0);
    v["puddles"] = json!("high"); // (apart: the literal above is at the json! macro's recursion limit)
    v["vehicle_smoke"] = json!("all");
}

pub fn settings_from_text_on(text: Option<&str>, phone: bool) -> Value {
    let mut v = json!({ "msaa": 4, "anisotropy": 8, "ssao": true, "shadows": true, "shadow_size": 2048, "navigator": true, "ui_opacity": 0.85, "navigator_corner": "bottom-left", "boarding": "auto", "detail_textures": true, "exact_fare": true, "enhanced": false, "graphics": "vanilla_plus", "fullscreen": false, "vsync": true, "volume": 0.6, "drive_keys": "simple", "render_scale": "auto", "view_distance": "auto", "language": "ENG", "video_memory": 0, "texture_compression": true, "gpu_texture_compression": true, "chat": true, "tooltips": true, "name_tags": true, "show_fps": false, "clouds": true, "cloud_quality": "high", "rain_quality": "high", "pax_density": 1.0, "vol_ai": 1.0, "vol_scenery": 1.0, "ambient": true, "vol_ambient": 0.8, "mirror_size": 256, "doppler": true, "driver": true, "max_fps": 0, "min_obj_size": 0.013, "max_obj_dist": "auto" });
    graphics_and_interface_defaults(&mut v);
    if phone {
        for (k, d) in [
            ("msaa", json!(2)),
            ("anisotropy", json!(4)),
            ("ssao", json!(false)),
            ("shadow_size", json!(1024)),
            ("mirror_size", json!(128)),
            ("max_fps", json!(60)),
            ("max_obj_dist", json!("900")),
            ("pax_density", json!(0.7)),
            ("navigator_corner", json!("top-center")),
        ] {
            v[k] = d;
        }
    }
    v["touch_door_mode"] = json!("standard");
    v["touch_pedal_length"] = json!(1.0);
    v["touch_pedal_deadzone"] = json!(0.15);
    // (the game's menus switch these: saved through this page's writer, unknown they were dropped)
    for k in ["full_tank", "trip_summary"] {
        v[k] = json!(true);
    }
    v["triple_screen"] = json!(false);
    v["triple_span"] = json!(true);
    v["triple_hud_center"] = json!(true);
    v["triple_fov_deg"] = json!(0.0);
    v["triple_width_mm"] = json!(600);
    v["triple_distance_mm"] = json!(650);
    v["triple_bezel_mm"] = json!(0);
    v["triple_left_angle_deg"] = json!(45);
    v["triple_right_angle_deg"] = json!(45);
    v["triple_eye_height_mm"] = json!(0);
    v["vr"] = json!(false);
    v["vr_scale"] = json!(0.65);
    v["vr_head_smoothing_ms"] = json!(0);
    v["vr_mirror_rate"] = json!(16);
    v["mirror_refresh"] = json!("full");
    v["mirror_quality"] = json!("standard");
    v["vr_desktop_mirror"] = json!(true);
    v["discord_status"] = json!(false); // (it tells friends the map, bus and line: only when asked)
    for k in [
        "discord_map",
        "discord_bus",
        "discord_line",
        "discord_stop",
        "discord_players",
        "discord_buttons",
    ] {
        v[k] = json!(true);
    }
    // Only fresh installs opt in; existing settings files retain enabled plugins.
    v["disable_plugins"] = json!(text.is_none());
    // a Tobii eye tracker read natively, beside opentrack (the game's menu switches these)
    v["head_tracking_source"] = json!("auto");
    v["tobii_edges"] = json!(false);
    v["head_tracking_smoothing"] = json!(0.5);
    v["head_tracking_dead_zone"] = json!(0.1);
    // iPhone, iPad, Mac: the cross-platform launcher (this one, drawn by the game) instead of the
    // system's own (SwiftUI); elsewhere there is only this one
    v["cross_platform_launcher"] = json!(false);
    v["discord_app_id"] = json!("");
    voice_defaults(&mut v);
    v["remote_cockpit"] = json!(false);
    v["remote_cockpit_port"] = json!(8642);
    // the launcher gives the graphics card up while a game runs (off: it stays drawn)
    v["launcher_rest"] = json!(true);
    // the window's size in pixels, "auto" to fit the screen (#904)
    v["resolution"] = json!("auto");
    // the game's information bar along the top, as the last session left it (#1164)
    v["info_bar"] = json!(false);
    v["perf_overlay"] = json!("off");
    for key in ["info_size", "timetable_size", "perf_size"] {
        v[key] = json!(1.0);
    }
    // Human posing; the original OMSI behavior remains the default.
    v["passenger_animation"] = json!("original");
    v["photo_mode"] = json!("simple");
    // the people as drawn: the content's own, or the Rocketbox avatars (this computer only)
    v["character_models"] = json!("original");
    v["units"] = json!("metric");
    // the trees' foliage bends and sways in the weather's wind
    v["windy_trees"] = json!(true);
    v["detailed_trees"] = json!(true);
    v["road_snow"] = json!(false); // opt-in; off preserves the weather's classic snow on roads
    v["sim_thread"] = json!(false); // (switched in the game's Display menu, saved through this writer)
    v["mx_graphics"] = json!(false);
    v["mirror_hud_every_frame"] = json!(false);
    v["mirror_shadows"] = json!(false);
    v["mirror_particles"] = json!(false);
    v["mirror_hud"] = json!(0);
    // an early timetable bus waits only at the stops the timetable times (off: at every
    // stop, as in OMSI; #1773)
    v["ai_wait_timed_stops_only"] = json!(false);
    v["pax_bad_driving"] = json!(true);
    passenger_defaults(&mut v);
    // the arrival half of the stops of a depot file that keeps two recordings per stop
    v["auto_announce"] = json!(false);
    // OMSI's own options
    for (k, d) in [
        ("maintenance", json!(0)),
        ("ai_unsched_factor", json!(100)),
        ("ai_reaction", json!(100)),
        ("standing_chance", json!(5)),
        ("ai_max_scheduled", json!(0)),
        ("ai_max_parked", json!(0)),
        ("ai_max_humans", json!(200)),
        ("parked_buses_drawn", json!(-1)),
        ("use_real_time", json!(false)),
        ("use_real_date", json!(false)),
        ("use_real_year", json!(false)),
        ("collision_vehicles", json!(true)),
        ("collision_objects", json!(true)),
        ("collision_pedestrians", json!(true)),
        ("realistic_road_physics", json!(false)),
        ("head_movement", json!(true)),
        ("driverview_smooth", json!(true)),
        ("hands_in_cab", json!(false)),
        ("driver_like_omsi", json!(false)),
        ("alt_view", json!(true)),
        ("precision_zoom", json!(false)),
    ] {
        v[k] = d;
    }
    // openOMSI's own: what passengers say, OMSI's route arrows, getting up from the seat
    for (k, d) in [
        ("pax_voices", json!("all")),
        ("nav_ai", json!(true)),
        ("nav_ai_cars", json!(true)),
        ("nav_surface", json!(true)),
        ("nav_light", json!(false)),
        ("nav_3d", json!(true)),
        ("nav_classic_2d", json!(false)),
        ("nav_widget", json!(true)),
        ("nav_size", json!(1.0)),
        ("get_up", json!(false)),
        ("time_speed", json!("1")),
        ("time_sync", json!(false)),
        ("metar_sync", json!(false)),
        ("metar_station", json!("")),
        ("machine_translation", json!(false)),
        ("shadow_casters", json!("all")),
        ("shadow_blobs", json!(true)),
        ("reflections", json!(1.0)),
        ("mouse_sens", json!(1.0)),
        ("graphics_api", json!("auto")),
        ("ctrl_off", json!("")),
        ("steering_linear", json!(false)),
        ("old_steering", json!(false)),
        ("red_steer_spd", json!(false)),
        ("ff_invert", json!(false)),
        ("ff_enabled", json!(true)),
        ("brake_hold", json!(true)),
        ("auto_clutch", json!(true)),
        ("momentary_gears", json!(false)),
        ("auto_shift", json!(false)),
        ("wheel_range", json!(900.0)),
        ("wheel_lock", json!(0.0)),
        ("cab_wheel_rotation", json!(0)),
        ("fov", json!(0.0)),
        ("camera_collision", json!(true)),
        ("right_stick_look", json!(true)),
        ("steer_look", json!(false)),
        ("pedal_throttle", json!(1.0)),
        ("pedal_brake", json!(1.0)),
        ("seat_x", json!(0.0)),
        ("seat_y", json!(0.0)),
        ("seat_z", json!(0.0)),
        ("seat_pitch_deg", json!(0.0)),
        ("head_tracking", json!(false)),
        ("head_tracking_yaw_sens", json!(100.0)),
        ("head_tracking_pitch_sens", json!(100.0)),
        ("head_tracking_roll_sens", json!(100.0)),
        ("head_tracking_x_sens", json!(100.0)),
        ("head_tracking_y_sens", json!(100.0)),
        ("head_tracking_z_sens", json!(100.0)),
        ("head_tracking_invert_yaw", json!(false)),
        ("head_tracking_invert_pitch", json!(false)),
        ("head_tracking_invert_roll", json!(false)),
        ("head_tracking_invert_x", json!(false)),
        ("head_tracking_invert_y", json!(false)),
        ("head_tracking_invert_z", json!(false)),
        ("led_glow", json!(6)),
        ("led_mips", json!(1.3)),
        ("night_brightness", json!(0.0)),
        ("ui_scale", json!(1.0)),
        ("ui_scale_window", json!(true)),
        ("chat_size", json!(1.0)),
        ("notes", json!(true)),
        ("notes_driving", json!(true)),
        ("notes_script", json!(true)),
        ("notes_trip", json!(true)),
        ("notes_tickets", json!(true)),
        ("mouse_steering", json!(false)),
        ("mouse_right_off", json!(false)),
        ("mouse_smooth", json!(true)),
        ("mouse_hold", json!(true)),
        ("mouse_pedals", json!(true)),
        ("key_pedal_speed", json!(1.0)),
        ("blinker_cancel", json!(true)),
        ("ff_road_vib", json!(1.0)),
        ("ff_engine_vib", json!(1.0)),
        ("ff_fade", json!(0.28)),
    ] {
        v[k] = d;
    }
    v["steer_look_angle"] = json!(30.0);
    v["look_sens"] = json!(1.0);
    v["look_smoothing_ms"] = json!(0.0);
    v["zoom_hold_amount"] = json!(0.5);
    v["zoom_hold_latch"] = json!(false);
    v["flashlight_cursor"] = json!(false);
    v["pad_steer_smooth"] = json!(120.0);
    v["pad_steer_linear"] = json!(false);
    v["ctrl_background"] = json!(false);
    v["pad_steer_speed"] = json!(2.0);
    v["pad_deadzone"] = json!(0.08);
    v["pad_type"] = json!("auto");
    v["pad_buttons"] = json!(true);
    v["arrows_switch_cams"] = json!(true);
    v["steer_look_response"] = json!(0.25);
    v["head_idle"] = json!(0.0);
    v["head_idle_pace"] = json!(1.0);
    // updates from the GitHub releases: looked for when the launcher starts (and during a
    // session, said over the navigator), installed after asking (or at once); and whether
    // the game is counted on the website's "playing now"
    for (k, d) in [
        ("update_check", json!(true)),
        ("update_auto", json!(false)),
        ("update_notify", json!(true)),
        ("update_channel", json!("stable")),
        ("presence", json!(true)),
        ("hardware_id", json!(true)),
    ] {
        v[k] = d;
    }
    let (dynamic_arrows, map_arrows) = route_arrows_from_text(text.unwrap_or_default());
    v["nav_dynamic_arrows"] = json!(dynamic_arrows);
    v["nav_map_arrows"] = json!(map_arrows);
    let Some(t) = text else { return v };
    v["ui_theme"] = json!(ui_theme_from_text(t));
    let mut version = 0;
    let mut graphics: Option<&str> = None;
    for line in t.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, val)) = line.split_once('=') else {
            continue;
        };
        let (k, val) = (setting_key(k), val.trim());
        let b = |x: &str| matches!(x.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
        match k.as_str() {
            "pause_quick_actions" | "ui_theme_pack" => v[&k] = json!(val),
            "anisotropy" => v[&k] = json!(val.parse::<i64>().unwrap_or(8).clamp(1, 16)),
            "msaa" | "shadow_size" => v[&k] = json!(val.parse::<i64>().unwrap_or(0)),
            "ui_opacity" | "volume" | "vol_ai" | "vol_scenery" | "vol_ambient" | "min_obj_size"
            | "radio_speaker" | "radio_hiss" | "radio_dropouts" => {
                v[&k] = json!(val.parse::<f64>().unwrap_or(0.0))
            }
            "pax_density" => {
                v[&k] = json!(
                    val.trim_end_matches('%')
                        .parse::<f64>()
                        .map(|x| if x > 5.0 { x / 100.0 } else { x })
                        .unwrap_or(1.0)
                )
            }
            "triple_fov_deg" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) })
                        .unwrap_or(0.0)
                )
            }
            "triple_width_mm" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(200.0, 2000.0))
                        .unwrap_or(600.0)
                )
            }
            "triple_distance_mm" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(200.0, 3000.0))
                        .unwrap_or(650.0)
                )
            }
            "triple_bezel_mm" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 100.0))
                        .unwrap_or(0.0)
                )
            }
            "triple_left_angle_deg" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 90.0))
                        .unwrap_or(45.0)
                )
            }
            "triple_right_angle_deg" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 90.0))
                        .unwrap_or(45.0)
                )
            }
            "triple_eye_height_mm" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(-500.0, 500.0))
                        .unwrap_or(0.0)
                )
            }
            "vr_scale" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.5, 1.0))
                        .unwrap_or(0.65)
                )
            }
            "vr_head_smoothing_ms" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 30.0) as i64)
                        .unwrap_or(0)
                )
            }
            "vr_mirror_rate" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(-1.0, 360.0) as i64)
                        .unwrap_or(16)
                )
            }
            "mirror_size" => {
                v[&k] = json!(
                    val.parse::<i64>()
                        .map(|x| if x == 0 { 0 } else { x.clamp(64, 2048) })
                        .unwrap_or(256)
                )
            }
            "mirror_refresh" => v[&k] = json!(mirror_refresh(val)),
            "mirror_quality" => v[&k] = json!(mirror_quality(val)),
            "cloud_quality" => v[&k] = json!(cloud_quality(val)),
            "street_light" => v[&k] = json!(street_light(val)),
            "ray_tracing" | "rt" => v["ray_tracing"] = json!(b(val)),
            "light_shadows" => v[&k] = json!(light_shadows(val)),
            "cloud_shadows" | "light_glow" => v[&k] = json!(b(val)),
            "rain_quality" => v[&k] = json!(rain_quality(val)),
            "vehicle_smoke" => v[&k] = json!(vehicle_smoke(val)),
            "puddles" => v[&k] = json!(puddles(val)),
            "upscaler" => v[&k] = json!(upscaler_word(val)),
            "upscale_quality" => v[&k] = json!(omsi_cfg::upscaler_files::quality_word(val)),
            "frame_generation" => {
                v[&k] = json!(omsi_cfg::upscaler_files::frame_generation_word(val))
            }
            "fsr_quality" => {
                v[&k] = json!(match val.to_ascii_lowercase().as_str() {
                    "ultra" => "ultra",
                    "balanced" => "balanced",
                    "performance" => "performance",
                    "custom" => "custom",
                    "auto" => "auto",
                    _ => "quality",
                })
            }
            "fsr_sharpness" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(0.8)
                        .clamp(0.0, 1.0)
                )
            }
            "gpu_texture_compression" => {
                v[&k] = json!(!matches!(
                    val.trim().to_ascii_lowercase().as_str(),
                    "0" | "off" | "no" | "false" | "disabled"
                ))
            }
            "max_fps" => v[&k] = json!(val.parse::<f64>().map(|x| x as i64).unwrap_or(0)),
            "max_obj_dist" => {
                v[&k] = if val.eq_ignore_ascii_case("auto") {
                    json!("auto")
                } else {
                    json!(
                        val.parse::<f64>()
                            .map(|m| (m.round() as i64).to_string())
                            .unwrap_or_else(|_| "auto".into())
                    )
                }
            }
            "ssao"
            | "shadows"
            | "shadow_blobs"
            | "lamp_shadows"
            | "navigator"
            | "enhanced"
            | "triple_screen"
            | "triple_span"
            | "triple_hud_center"
            | "vr"
            | "vr_desktop_mirror"
            | "fullscreen"
            | "vsync"
            | "exact_fare"
            | "detail_textures"
            | "texture_compression"
            | "chat"
            | "tooltips"
            | "name_tags"
            | "show_fps"
            | "clouds"
            | "doppler"
            | "driver"
            | "use_real_time"
            | "use_real_date"
            | "use_real_year"
            | "collision_vehicles"
            | "collision_objects"
            | "collision_pedestrians"
            | "realistic_road_physics"
            | "head_movement"
            | "driverview_smooth"
            | "hands_in_cab"
            | "alt_view"
            | "precision_zoom" => v[&k] = json!(b(val)),
            "maintenance" | "ai_unsched_factor" | "ai_reaction" | "ai_max_scheduled"
            | "standing_chance" => {
                v[&k] = json!(
                    val.trim_end_matches('%')
                        .parse::<f64>()
                        .map(|x| x.max(0.0) as i64)
                        .unwrap_or(0)
                )
            }
            // (-1: no parked cars at all, #864)
            "ai_max_parked" => {
                v[&k] = json!(val.parse::<f64>().map(|x| x.max(-1.0) as i64).unwrap_or(0))
            }
            "ai_max_humans" => {
                v[&k] = json!(val.parse::<f64>().map(|x| x.max(1.0) as i64).unwrap_or(200))
            }
            "parked_buses_drawn" => {
                v[&k] = json!(val.parse::<f64>().map(|x| x.max(-1.0) as i64).unwrap_or(-1))
            }
            "drive_keys" | "navigator_corner" | "boarding" | "render_scale" | "pax_voices" => {
                v[&k] = json!(val)
            }
            "touch_door_mode" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("direct") {
                    "direct"
                } else {
                    "standard"
                })
            }
            "update_channel" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("beta") {
                    "beta"
                } else {
                    "stable"
                })
            }
            "photo_mode" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("advanced") {
                    "advanced"
                } else {
                    "simple"
                })
            }
            "passenger_transfer_percent" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(20.0)
                        .clamp(0.0, 100.0)
                )
            }
            "passenger_animation" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("enhanced") {
                    "enhanced"
                } else {
                    "original"
                })
            }
            "character_models" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("improved") {
                    "improved"
                } else {
                    "original"
                })
            }
            "units" => v[&k] = json!(units_setting(val)),
            "ctrl_off" => v[&k] = json!(val),
            "metar_station" => {
                v[&k] = json!(
                    val.chars()
                        .filter(|c| c.is_ascii_alphabetic())
                        .take(4)
                        .collect::<String>()
                        .to_ascii_uppercase()
                )
            }
            "discord_app_id" => v[&k] = json!(val),
            "voice_mode" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("open") {
                    "open"
                } else {
                    "ptt"
                })
            }
            "voice_input_device" => v[&k] = json!(val),
            "voice_mic_gain" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 4.0))
                        .unwrap_or(1.0)
                )
            }
            "voice_volume" | "voice_volume_nearby" | "voice_volume_radio" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 2.0))
                        .unwrap_or(1.0)
                )
            }
            "voice_threshold_db" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(-80.0, -10.0).round() as i64)
                        .unwrap_or(-45)
                )
            }
            "voice_range" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(5.0, 1000.0).round() as i64)
                        .unwrap_or(75)
                )
            }
            "voice_enabled" | "voice_talker_list" | "voice_all_off" => v[&k] = json!(b(val)),
            "resolution" | "window_size" => v["resolution"] = json!(resolution_text(val)),
            "graphics_api" => {
                v[&k] = json!(match val.to_ascii_lowercase().as_str() {
                    "vulkan" => "vulkan",
                    "dx12" => "dx12",
                    "gl" => "gl",
                    "angle" => "angle",
                    _ => "auto",
                })
            }
            "shadow_casters" => {
                v[&k] = json!(if val.eq_ignore_ascii_case("omsi") {
                    "omsi"
                } else {
                    "all"
                })
            }
            "ctrl_deadzone" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.0).clamp(0.0, 0.3)),
            "mouse_sens" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.1, 3.0)),
            "key_pedal_speed" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.25, 2.0)),
            "touch_pedal_length" => {
                v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.8, 1.6))
            }
            "touch_pedal_deadzone" => {
                v[&k] = json!(val.parse::<f64>().unwrap_or(0.15).clamp(0.0, 0.4))
            }
            "mouse_pedal_strength" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.25, 4.0)
                )
            }
            "ui_scale" | "nav_size" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(if k == "ui_scale" { 0.5 } else { 0.6 }, 2.0)
                )
            }
            "chat_size" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.5, 3.0)
                )
            }
            "wheel_range" => v[&k] = json!(val.parse::<f64>().unwrap_or(900.0).clamp(90.0, 2880.0)),
            "wheel_lock" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .map(|x| if x < 45.0 { 0.0 } else { x.min(2880.0) })
                        .unwrap_or(0.0)
                )
            }
            // (the cab's steering wheel model, degrees lock to lock; 0 = the bus's own animation)
            "cab_wheel_rotation" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite() && *x >= 90.0)
                        .map(|x| (x.round() as i64).clamp(180, 1800))
                        .unwrap_or(0)
                )
            }
            "fov" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) })
                        .unwrap_or(0.0)
                )
            }
            "pad_steer_speed" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.8, 5.0))
                        .unwrap_or(2.0)
                )
            }
            "pad_deadzone" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 0.4))
                        .unwrap_or(0.08)
                )
            }
            "pad_type" => {
                v[&k] = json!(match val.trim().to_ascii_lowercase().as_str() {
                    "xbox" => "xbox",
                    "ps4" => "ps4",
                    "ps5" => "ps5",
                    _ => "auto",
                })
            }
            "camera_collision"
            | "right_stick_look"
            | "ctrl_background"
            | "pad_steer_linear"
            | "pad_buttons"
            | "arrows_switch_cams"
            | "steer_look"
            | "head_tracking"
            | "head_tracking_invert_yaw"
            | "head_tracking_invert_pitch"
            | "head_tracking_invert_roll"
            | "head_tracking_invert_x"
            | "head_tracking_invert_y"
            | "head_tracking_invert_z"
            | "discord_status"
            | "discord_map"
            | "discord_bus"
            | "discord_line"
            | "discord_stop"
            | "discord_players"
            | "discord_buttons"
            | "remote_cockpit"
            | "launcher_rest"
            | "cross_platform_launcher" => v[&k] = json!(b(val)),
            // (how much of the mip chain an LED panel is held at, 0..4; a file from before
            // it was a number says 1 or 0)
            "led_mips" => {
                v[&k] = json!(
                    val.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 4.0))
                        .unwrap_or(1.3)
                )
            }
            "led_glow" => v[&k] = json!(val.parse::<i64>().map(|x| x.clamp(0, 15)).unwrap_or(6)),
            "remote_cockpit_port" => {
                v[&k] = json!(
                    val.trim()
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p > 0)
                        .unwrap_or(8642)
                )
            }
            "night_brightness" => {
                v[&k] = json!(
                    val.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 3.0))
                        .unwrap_or(0.0)
                )
            }
            "reflections" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map_or_else(|| if b(val) { 1.0 } else { 0.0 }, |x| x.clamp(0.0, 1.0))
                )
            }
            "road_glare" | "traffic_headlights" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.0, 2.0)
                )
            }
            "look_sens" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.1, 2.0))
                        .unwrap_or(1.0)
                )
            }
            "look_smoothing_ms" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 200.0))
                        .unwrap_or(0.0)
                )
            }
            "zoom_hold_amount" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.15, 1.0))
                        .unwrap_or(0.5)
                )
            }
            "zoom_hold_latch" => v[&k] = json!(b(val)),
            "flashlight_cursor" => v[&k] = json!(b(val)),
            "pad_steer_smooth" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 300.0))
                        .unwrap_or(120.0)
                )
            }
            "steer_look_angle" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 60.0))
                        .unwrap_or(30.0)
                )
            }
            "steer_look_response" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.05, 1.0))
                        .unwrap_or(0.25)
                )
            }
            "head_idle" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 1.0))
                        .unwrap_or(0.0)
                )
            }
            "head_idle_pace" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.5, 2.0))
                        .unwrap_or(1.0)
                )
            }
            "pedal_throttle" | "pedal_brake" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .map(|x| x.clamp(0.25, 4.0))
                        .unwrap_or(1.0)
                )
            }
            "ff_road_vib" | "ff_engine_vib" => {
                v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.0, 4.0)).unwrap_or(1.0))
            }
            "ff_fade" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .map(|x| x.clamp(0.0, 1.5))
                        .unwrap_or(0.28)
                )
            }
            "seat_x" | "seat_y" | "seat_z" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .map(|x| x.clamp(-1.5, 1.5))
                        .unwrap_or(0.0)
                )
            }
            "seat_pitch_deg" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(0.0)
                        .clamp(-45.0, 45.0)
                )
            }
            "head_tracking_yaw_sens"
            | "head_tracking_pitch_sens"
            | "head_tracking_roll_sens"
            | "head_tracking_x_sens"
            | "head_tracking_y_sens"
            | "head_tracking_z_sens" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0.0, 100.0))
                        .unwrap_or(100.0)
                )
            }
            "full_tank" | "trip_summary" => v[&k] = json!(b(val)),
            "nav_ai"
            | "nav_ai_cars"
            | "nav_surface"
            | "nav_3d"
            | "nav_widget"
            | "get_up"
            | "time_sync"
            | "metar_sync"
            | "ui_scale_window"
            | "notes"
            | "notes_driving"
            | "notes_script"
            | "notes_trip"
            | "notes_tickets"
            | "machine_translation"
            | "update_check"
            | "update_auto"
            | "update_notify"
            | "presence"
            | "hardware_id"
            | "steering_linear"
            | "old_steering"
            | "red_steer_spd"
            | "ff_invert"
            | "ff_enabled"
            | "brake_hold"
            | "auto_clutch"
            | "momentary_gears"
            | "auto_shift"
            | "mouse_steering"
            | "mouse_right_off"
            | "mouse_smooth"
            | "mouse_hold"
            | "mouse_pedals"
            | "blinker_cancel" => v[&k] = json!(b(val)),
            "nav_light" => v[&k] = json!(b(val)),
            "nav_classic_2d" => v[&k] = json!(b(val)),
            "info_bar" | "disable_plugins" => v[&k] = json!(b(val)),
            "info_size" | "timetable_size" | "perf_size" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.6, 2.0)
                )
            }
            "perf_overlay" => v[&k] = json!(perf_overlay_mode(val)),
            "sim_thread" => v[&k] = json!(b(val)),
            "mx_graphics" => v[&k] = json!(b(val)),
            "mirror_hud" => v[&k] = json!(val.trim().parse::<u8>().map(|x| x.min(7)).unwrap_or(0)),
            "windy_trees"
            | "detailed_trees"
            | "road_snow"
            | "mirror_hud_every_frame"
            | "mirror_shadows"
            | "mirror_particles"
            | "ai_wait_timed_stops_only"
            | "pax_bad_driving"
            | "passenger_connections"
            | "passenger_hailing"
            | "passenger_full_bus_reaction"
            | "pax_short_rides_stand"
            | "auto_announce"
            | "ambient" => v[&k] = json!(b(val)),
            "head_tracking_source" => v[&k] = json!(head_tracking_source(val)),
            "tobii_edges" => v[&k] = json!(b(val)),
            "head_tracking_smoothing" | "head_tracking_dead_zone" => {
                v[&k] = json!(head_tracking_value(&k, val.parse::<f64>().ok()))
            }
            "time_speed" => {
                v[&k] = json!(
                    val.trim_start_matches(['x', 'X'])
                        .parse::<f64>()
                        .map(|x| x.clamp(1.0, 30.0))
                        .map(|x| if x.fract() == 0.0 {
                            format!("{}", x as i64)
                        } else {
                            x.to_string()
                        })
                        .unwrap_or_else(|_| "1".into())
                )
            }
            "language" => v[&k] = json!(language_code(val)),
            "graphics" | "renderer" => graphics = Some(graphics_mode(val)),
            // whole metres, as the page's select has them ("1500"); anything else (auto) is
            // the game's default
            "view_distance" => {
                v[&k] = match val.parse::<f64>() {
                    Ok(m) if m > 0.0 => json!((m.round() as i64).to_string()),
                    _ => json!("auto"),
                }
            }
            // MB, 0 = automatic (a fraction written by hand is cut, as the game does)
            "video_memory" => {
                if let Ok(mb) = val.parse::<f64>() {
                    v[&k] = json!(mb.max(0.0) as i64);
                }
            }
            "version" => version = val.parse::<i64>().unwrap_or(0),
            _ => {}
        }
    }
    // a file without `graphics` (older builds) says only `enhanced`; its vanilla renderer is
    // what is now Vanilla+
    let g = graphics.unwrap_or(if v["enhanced"] == json!(true) {
        "enhanced"
    } else {
        "vanilla_plus"
    });
    // (a file of the days of Enhanced+ names it as a mode: it is Enhanced with ray tracing on)
    let g = if g == "enhanced_plus" {
        v["ray_tracing"] = json!(true);
        "enhanced"
    } else {
        g
    };
    v["graphics"] = json!(g);
    v["enhanced"] = json!(g == "enhanced");
    // before version 2 the launcher wrote its old default `boarding=pay` for everybody
    // (passengers then waited at the cash desk for the driver): the game reads that as auto
    if version < 2 && v["boarding"] == "pay" {
        v["boarding"] = json!("auto");
    }
    if version == 3 && v["passenger_animation"] == "enhanced" {
        v["passenger_animation"] = json!("original");
    }
    v
}

/// OMSI's tutorials (`Tutorials/menu_<n>_<LANG>.html`): per lesson its title and what
/// it teaches, in the settings' language.
pub fn tutorials() -> Vec<(usize, String, String)> {
    let Ok(r) = root() else { return Vec::new() };
    let lang = content_language();
    let mut out = Vec::new();
    for n in 1..=4usize {
        let p = [lang, "ENG", "DEU"]
            .iter()
            .map(|l| r.join("Tutorials").join(format!("menu_{n}_{l}.html")))
            .find(|p| p.is_file());
        let Some(p) = p else { continue };
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        let html = omsi_cfg::codepage::decode(&bytes);
        let body = html.split_once("</style>").map(|x| x.1).unwrap_or(&html);
        let mut text = String::new();
        let mut tag = false;
        for c in body
            .replace("</p>", "\n")
            .replace("<br>", "\n")
            .replace("</h2>", "\n")
            .replace("<li>", "\n• ")
            .chars()
        {
            match c {
                '<' => tag = true,
                '>' => tag = false,
                _ if !tag => text.push(c),
                _ => {}
            }
        }
        let text = decode_html_entities(&text);
        let mut lines = text
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty());
        let title = lines.next().unwrap_or_default();
        let rest: Vec<String> = lines.collect();
        out.push((n, title, rest.join("\n")));
    }
    out
}

pub use omsi_content::tutorial::decode_html_entities;

/// OMSI's option presets (`option_presets/*.oop`): their names and what they say, in the
/// settings' own keys (maxFPS, performance_minObjSize/maxObjDist, texFilter, texmemlimit,
/// performance_reflTexSize).
pub fn option_presets() -> Vec<(String, Value)> {
    let Ok(r) = root() else { return Vec::new() };
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir(r.join("option_presets")) else {
        return out;
    };
    let mut files: Vec<PathBuf> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("oop"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    for f in files {
        let Ok(o) = omsi_content::options::Options::load(&f) else {
            continue;
        };
        let name = f
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut v = json!({});
        if !cfg!(any(target_os = "android", target_os = "ios")) {
            v["max_fps"] = json!(o.i32("maxfps", 0).max(0));
        }
        v["min_obj_size"] = json!(o.f32("performance_minobjsize", 0.013) as f64);
        v["max_obj_dist"] =
            json!((o.f32("performance_maxobjdist", 900.0).round() as i64).to_string());
        if let Some(af) = o
            .values
            .get("texfilter")
            .and_then(|x| x.get(1))
            .and_then(|x| x.parse::<i64>().ok())
        {
            v["anisotropy"] = json!(af.clamp(1, 16));
        }
        let refl = o.i32("performance_refltexsize", 8);
        v["mirror_size"] = json!(1i64 << refl.clamp(6, 11));
        if let Some(x) = o.str("performance_realreflexions") {
            v["mirror_refresh"] = json!(mirror_refresh(x));
        }
        out.push((name, v));
    }
    out
}

pub fn save_settings(v: &Value) -> Result<()> {
    let p = data_dir().join("settings.cfg");
    let old = std::fs::read_to_string(&p).ok();
    std::fs::write(p, settings_to_text(v, old.as_deref()))?;
    Ok(())
}

/// The `settings.cfg` text for the page's values `v`; the lines of the `old` file that the
/// page does not manage (keys of newer games, hand-written switches) are kept.
/// The `units` setting's value: `metric`, `uk` or `us` (the game's `units` module).
fn units_setting(v: &str) -> &'static str {
    match v.trim().to_ascii_lowercase().as_str() {
        "uk" | "british" => "uk",
        "us" | "american" | "imperial" => "us",
        _ => "metric",
    }
}

pub fn settings_to_text(v: &Value, old: Option<&str>) -> String {
    let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d) as u8;
    // a number, also as the string a select gives
    let n = |k: &str, d: i64| {
        v.get(k)
            .and_then(|x| {
                x.as_i64().or_else(|| {
                    x.as_f64()
                        .or_else(|| x.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
                        .map(|f| f as i64)
                })
            })
            .unwrap_or(d)
    };
    let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
    let legacy_arrows = b("nav_arrows", false) != 0;
    let dynamic_arrows = b("nav_dynamic_arrows", legacy_arrows);
    let map_arrows = b("nav_map_arrows", legacy_arrows);
    let text = format!(
        "# openOMSI settings (written by the launcher)\nversion=4\nmsaa={}\nanisotropy={}\nssao={}\nshadows={}\nshadow_size={}\nnavigator={}\nui_opacity={}\nnavigator_corner={}\nboarding={}\ndetail_textures={}\nexact_fare={}\nenhanced={}\ngraphics={}\nfullscreen={}\nvsync={}\nvolume={}\ndrive_keys={}\nrender_scale={}\nview_distance={}\nlanguage={}\nvideo_memory={}\ntexture_compression={}\nchat={}\ntooltips={}\nname_tags={}\nshow_fps={}\nclouds={}\npax_density={}\nvol_ai={}\nvol_scenery={}\nmirror_size={}\ndoppler={}\ndriver={}\nmax_fps={}\nmin_obj_size={}\nmax_obj_dist={}\n",
        n("msaa", 4),
        n("anisotropy", 8),
        b("ssao", true),
        b("shadows", true),
        n("shadow_size", 2048),
        b("navigator", true),
        f("ui_opacity", 0.85),
        v.get("navigator_corner")
            .and_then(|x| x.as_str())
            .unwrap_or("bottom-left"),
        v.get("boarding").and_then(|x| x.as_str()).unwrap_or("auto"),
        b("detail_textures", true),
        b("exact_fare", true),
        matches!(
            graphics_mode(
                v.get("graphics")
                    .and_then(|x| x.as_str())
                    .unwrap_or("vanilla_plus")
            ),
            "enhanced" | "enhanced_plus"
        ) as u8,
        match graphics_mode(
            v.get("graphics")
                .and_then(|x| x.as_str())
                .unwrap_or("vanilla_plus")
        ) {
            "enhanced_plus" => "enhanced",
            g => g,
        },
        b("fullscreen", false),
        b("vsync", true),
        f("volume", 0.6),
        v.get("drive_keys")
            .and_then(|x| x.as_str())
            .unwrap_or("simple"),
        // "auto" or a fraction, as a string (the page's select) or a number
        match v.get("render_scale") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(Value::Number(x)) => x.to_string(),
            _ => "auto".to_string(),
        },
        // metres, or "auto" (the game's 1200 m)
        match v.get("view_distance") {
            Some(Value::String(s)) if s.trim().parse::<f64>().map(|m| m > 0.0).unwrap_or(false) =>
                s.trim().to_string(),
            Some(Value::Number(x)) if x.as_f64().map(|m| m > 0.0).unwrap_or(false) => x.to_string(),
            _ => "auto".to_string(),
        },
        language_code(v.get("language").and_then(|x| x.as_str()).unwrap_or("ENG")),
        n("video_memory", 0).max(0),
        b("texture_compression", true),
        b("chat", true),
        b("tooltips", true),
        b("name_tags", true),
        b("show_fps", false),
        b("clouds", true),
        f("pax_density", 1.0),
        f("vol_ai", 1.0),
        f("vol_scenery", 1.0),
        match n("mirror_size", 256) {
            0 => 0,
            x => x.clamp(64, 2048),
        },
        b("doppler", true),
        b("driver", true),
        n("max_fps", 0).max(0),
        f("min_obj_size", 0.013),
        match v.get("max_obj_dist") {
            Some(Value::String(s)) if s.trim().parse::<f64>().is_ok() => s.trim().to_string(),
            Some(Value::Number(x)) => x.to_string(),
            _ => "auto".to_string(),
        },
    );
    let ui_theme = if v
        .get("ui_theme")
        .and_then(|x| x.as_str())
        .is_some_and(|x| x.eq_ignore_ascii_case("light"))
    {
        "light"
    } else {
        "dark"
    };
    let text = format!("{text}ui_theme={ui_theme}\n");
    // OMSI's own options (options.cfg): maintenance ([wear_lifespan]), the AI counts and
    // the share of random traffic, the real clock and calendar, collisions, head movement
    let text = format!(
        "{text}maintenance={}\nai_unsched_factor={}\nai_reaction={}\nai_max_scheduled={}\nai_max_parked={}\nai_max_humans={}\nparked_buses_drawn={}\nuse_real_time={}\nuse_real_date={}\nuse_real_year={}\ncollision_vehicles={}\ncollision_objects={}\ncollision_pedestrians={}\nrealistic_road_physics={}\nhead_movement={}\ndriverview_smooth={}\nhands_in_cab={}\ndriver_like_omsi={}\nalt_view={}\nprecision_zoom={}\n",
        n("maintenance", 0).clamp(0, 4),
        n("ai_unsched_factor", 100).clamp(0, 300),
        n("ai_reaction", 100).clamp(25, 200),
        n("ai_max_scheduled", 0).max(0),
        n("ai_max_parked", 0).max(-1),
        n("ai_max_humans", 200).max(1),
        n("parked_buses_drawn", -1).max(-1),
        b("use_real_time", false),
        b("use_real_date", false),
        b("use_real_year", false),
        b("collision_vehicles", true),
        b("collision_objects", true),
        b("collision_pedestrians", true),
        b("realistic_road_physics", false),
        b("head_movement", true),
        b("driverview_smooth", true),
        b("hands_in_cab", false),
        b("driver_like_omsi", false),
        b("alt_view", true),
        b("precision_zoom", false),
    );
    let text = format!(
        "{text}nav_dynamic_arrows={dynamic_arrows}\nnav_map_arrows={map_arrows}\npax_voices={}\nnav_ai={}\nnav_ai_cars={}\nnav_surface={}\nnav_3d={}\nnav_classic_2d={}\nnav_widget={}\nnav_size={}\nget_up={}\ntime_speed={}\nmachine_translation={}\nshadow_casters={}\nshadow_blobs={}\nlamp_shadows={}\nctrl_deadzone={}\nupdate_check={}\nupdate_auto={}\nupdate_notify={}\npresence={}\nhardware_id={}\nreflections={}\nmouse_sens={}\nmouse_pedal_strength={}\ngraphics_api={}\nctrl_off={}\nsteering_linear={}\nold_steering={}\nred_steer_spd={}\nff_invert={}\nwheel_range={}\nwheel_lock={}\ncab_wheel_rotation={}\nfov={}\ncamera_collision={}\npedal_throttle={}\npedal_brake={}\nseat_x={}\nseat_y={}\nseat_z={}\nseat_pitch_deg={}\nsteer_look={}\nhead_tracking={}\nhead_tracking_yaw_sens={}\nhead_tracking_pitch_sens={}\nhead_tracking_roll_sens={}\nhead_tracking_x_sens={}\nhead_tracking_y_sens={}\nhead_tracking_z_sens={}\nhead_tracking_invert_yaw={}\nhead_tracking_invert_pitch={}\nhead_tracking_invert_roll={}\nhead_tracking_invert_x={}\nhead_tracking_invert_y={}\nhead_tracking_invert_z={}\nff_enabled={}\nbrake_hold={}\nauto_clutch={}\nmomentary_gears={}\nauto_shift={}\nled_glow={}\nled_mips={}\nnight_brightness={}\nui_scale={}\nui_scale_window={}\nchat_size={}\nnotes={}\nnotes_driving={}\nnotes_script={}\nnotes_trip={}\nnotes_tickets={}\nmouse_steering={}\nmouse_right_off={}\nmouse_smooth={}\nmouse_hold={}\nblinker_cancel={}\nff_road_vib={}\nff_engine_vib={}\nff_fade={}\n",
        match v
            .get("pax_voices")
            .and_then(|x| x.as_str())
            .unwrap_or("all")
        {
            "tickets" => "tickets",
            "off" => "off",
            _ => "all",
        },
        b("nav_ai", true),
        b("nav_ai_cars", true),
        b("nav_surface", true),
        b("nav_3d", true),
        b("nav_classic_2d", false),
        b("nav_widget", true),
        f("nav_size", 1.0).clamp(0.6, 2.0),
        b("get_up", false),
        match v.get("time_speed") {
            Some(Value::String(x)) => x
                .trim()
                .parse::<f64>()
                .map(|x| x.clamp(1.0, 30.0))
                .unwrap_or(1.0),
            Some(Value::Number(x)) => x.as_f64().unwrap_or(1.0).clamp(1.0, 30.0),
            _ => 1.0,
        },
        b("machine_translation", false),
        if v.get("shadow_casters").and_then(|x| x.as_str()) == Some("omsi") {
            "omsi"
        } else {
            "all"
        },
        b("shadow_blobs", true),
        b("lamp_shadows", true),
        f("ctrl_deadzone", 0.0).clamp(0.0, 0.3),
        b("update_check", true),
        b("update_auto", false),
        b("update_notify", true),
        b("presence", true),
        b("hardware_id", true),
        v.get("reflections")
            .and_then(|x| x
                .as_f64()
                .or_else(|| x.as_bool().map(|on| if on { 1.0 } else { 0.0 })))
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
        f("mouse_sens", 1.0).clamp(0.1, 3.0),
        f("mouse_pedal_strength", 1.0).clamp(0.25, 4.0),
        match v
            .get("graphics_api")
            .and_then(|x| x.as_str())
            .unwrap_or("auto")
        {
            "vulkan" => "vulkan",
            "dx12" => "dx12",
            "gl" => "gl",
            "angle" => "angle",
            _ => "auto",
        },
        v.get("ctrl_off")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .replace(['\n', '\r'], " "),
        b("steering_linear", false),
        b("old_steering", false),
        b("red_steer_spd", false),
        b("ff_invert", false),
        f("wheel_range", 900.0).clamp(90.0, 2880.0),
        f("wheel_lock", 0.0).clamp(0.0, 2880.0),
        match n("cab_wheel_rotation", 0) {
            x if x < 90 => 0,
            x => x.clamp(180, 1800),
        },
        f("fov", 0.0).clamp(0.0, 120.0),
        b("camera_collision", true),
        f("pedal_throttle", 1.0).clamp(0.25, 4.0),
        f("pedal_brake", 1.0).clamp(0.25, 4.0),
        f("seat_x", 0.0).clamp(-1.5, 1.5),
        f("seat_y", 0.0).clamp(-1.5, 1.5),
        f("seat_z", 0.0).clamp(-1.5, 1.5),
        f("seat_pitch_deg", 0.0).clamp(-45.0, 45.0),
        b("steer_look", false),
        b("head_tracking", false),
        f("head_tracking_yaw_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_pitch_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_roll_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_x_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_y_sens", 100.0).clamp(0.0, 100.0),
        f("head_tracking_z_sens", 100.0).clamp(0.0, 100.0),
        b("head_tracking_invert_yaw", false),
        b("head_tracking_invert_pitch", false),
        b("head_tracking_invert_roll", false),
        b("head_tracking_invert_x", false),
        b("head_tracking_invert_y", false),
        b("head_tracking_invert_z", false),
        b("ff_enabled", true),
        b("brake_hold", true),
        b("auto_clutch", true),
        b("momentary_gears", false),
        b("auto_shift", false),
        n("led_glow", 6).clamp(0, 15),
        f("led_mips", 1.3).clamp(0.0, 4.0),
        f("night_brightness", 0.0).clamp(0.0, 3.0),
        f("ui_scale", 1.0).clamp(0.5, 2.0),
        b("ui_scale_window", true),
        f("chat_size", 1.0).clamp(0.5, 3.0),
        b("notes", true),
        b("notes_driving", true),
        b("notes_script", true),
        b("notes_trip", true),
        b("notes_tickets", true),
        b("mouse_steering", false),
        b("mouse_right_off", false),
        b("mouse_smooth", true),
        b("mouse_hold", true),
        b("blinker_cancel", true),
        f("ff_road_vib", 1.0).clamp(0.0, 4.0),
        f("ff_engine_vib", 1.0).clamp(0.0, 4.0),
        f("ff_fade", 0.28).clamp(0.0, 1.5),
    );
    let vr_scale = v
        .get("vr_scale")
        .and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
        .filter(|x| x.is_finite())
        .unwrap_or(0.65)
        .clamp(0.5, 1.0);
    let vr_head_smoothing_ms = v
        .get("vr_head_smoothing_ms")
        .and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
        .filter(|x| x.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 30.0);
    let vr_mirror_rate = v
        .get("vr_mirror_rate")
        .and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
        .filter(|x| x.is_finite())
        .unwrap_or(16.0)
        .clamp(-1.0, 360.0);
    let remote_port = v
        .get("remote_cockpit_port")
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .filter(|p| (1..=65535).contains(p))
        .unwrap_or(8642);
    let text = format!(
        "{text}vr={}\nvr_scale={vr_scale}\nvr_head_smoothing_ms={vr_head_smoothing_ms}\nvr_mirror_rate={vr_mirror_rate}\nvr_desktop_mirror={}\ndiscord_status={}\ndiscord_map={}\ndiscord_bus={}\ndiscord_line={}\ndiscord_stop={}\ndiscord_players={}\ndiscord_buttons={}\nremote_cockpit={}\nremote_cockpit_port={remote_port}\nlauncher_rest={}\ncross_platform_launcher={}\n",
        b("vr", false),
        b("vr_desktop_mirror", true),
        b("discord_status", false),
        b("discord_map", true),
        b("discord_bus", true),
        b("discord_line", true),
        b("discord_stop", true),
        b("discord_players", true),
        b("discord_buttons", true),
        b("remote_cockpit", false),
        b("launcher_rest", true),
        b("cross_platform_launcher", false)
    );
    // what the page does not manage (keys of newer games, hand-written ones) stays as it
    // was in the file; other spellings of the keys just written go
    let mut text = text;
    text.push_str(&format!(
        "right_stick_look={}\n",
        b("right_stick_look", true)
    ));
    text.push_str(&format!(
        "pad_steer_linear={}\n",
        b("pad_steer_linear", false)
    ));
    text.push_str(&format!(
        "ctrl_background={}\n",
        b("ctrl_background", false)
    ));
    text.push_str(&format!(
        "pad_steer_speed={}\npad_deadzone={}\npad_buttons={}\n",
        f("pad_steer_speed", 2.0).clamp(0.8, 5.0),
        f("pad_deadzone", 0.08).clamp(0.0, 0.4),
        b("pad_buttons", true)
    ));
    text.push_str(&format!(
        "pad_type={}\n",
        match v.get("pad_type").and_then(|x| x.as_str()).unwrap_or("auto") {
            t @ ("xbox" | "ps4" | "ps5") => t,
            _ => "auto",
        }
    ));
    text.push_str(&format!(
        "touch_door_mode={}\n",
        if v.get("touch_door_mode").and_then(|x| x.as_str()) == Some("direct") {
            "direct"
        } else {
            "standard"
        }
    ));
    text.push_str(&format!(
        "update_channel={}\n",
        if v.get("update_channel").and_then(|x| x.as_str()) == Some("beta") {
            "beta"
        } else {
            "stable"
        }
    ));
    text.push_str(&format!(
        "arrows_switch_cams={}\n",
        b("arrows_switch_cams", true)
    ));
    text.push_str(&format!("disable_plugins={}\n", b("disable_plugins", true)));
    // voice chat (the game clamps again when it reads them)
    {
        let text_of = |k: &str, d: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .map(|x| x.replace(['\n', '\r'], " ").trim().to_string())
                .unwrap_or_else(|| d.to_string())
        };
        text.push_str(&format!(
            "voice_enabled={}\nvoice_mode={}\nvoice_mic_gain={}\nvoice_threshold_db={}\nvoice_input_device={}\nvoice_volume={}\nvoice_volume_nearby={}\nvoice_volume_radio={}\nvoice_range={}\nvoice_talker_list={}\nvoice_all_off={}\n",
            b("voice_enabled", false),
            if text_of("voice_mode", "ptt") == "open" { "open" } else { "ptt" },
            f("voice_mic_gain", 1.0).clamp(0.0, 4.0),
            n("voice_threshold_db", -45).clamp(-80, -10),
            text_of("voice_input_device", ""),
            f("voice_volume", 1.0).clamp(0.0, 2.0),
            f("voice_volume_nearby", 1.0).clamp(0.0, 2.0),
            f("voice_volume_radio", 1.0).clamp(0.0, 2.0),
            n("voice_range", 75).clamp(5, 1000),
            b("voice_talker_list", true),
            b("voice_all_off", false),
        ));
    }
    text.push_str(&format!(
        "pause_quick_actions={}\n",
        v.get("pause_quick_actions")
            .and_then(|x| x.as_str())
            .unwrap_or("default")
            .replace(['\r', '\n'], "")
    ));
    text.push_str(&format!(
        "ui_theme_pack={}\n",
        v.get("ui_theme_pack")
            .and_then(|x| x.as_str())
            .filter(|x| !x.trim().is_empty())
            .unwrap_or("openomsi")
            .replace(['\r', '\n'], "")
    ));
    text.push_str(&format!("mouse_pedals={}\n", b("mouse_pedals", true)));
    text.push_str(&format!(
        "key_pedal_speed={}\n",
        f("key_pedal_speed", 1.0).clamp(0.25, 2.0)
    ));
    text.push_str(&format!(
        "touch_pedal_length={}\ntouch_pedal_deadzone={}\n",
        f("touch_pedal_length", 1.0).clamp(0.8, 1.6),
        f("touch_pedal_deadzone", 0.15).clamp(0.0, 0.4)
    ));
    text.push_str(&format!(
        "resolution={}\n",
        resolution_text(
            v.get("resolution")
                .and_then(|x| x.as_str())
                .unwrap_or("auto")
        )
    ));
    text.push_str(&format!(
        "gpu_texture_compression={}\n",
        b("gpu_texture_compression", true)
    ));
    text.push_str(&format!(
        "cloud_quality={}\n",
        cloud_quality(
            v.get("cloud_quality")
                .and_then(|x| x.as_str())
                .unwrap_or("high")
        )
    ));
    text.push_str(&format!(
        "street_light={}\n",
        street_light(
            v.get("street_light")
                .and_then(|x| x.as_str())
                .unwrap_or("off")
        )
    ));
    text.push_str(&format!(
        "ray_tracing={}\nlight_shadows={}\ncloud_shadows={}\nlight_glow={}\n",
        b("ray_tracing", false)
            | (graphics_mode(
                v.get("graphics")
                    .and_then(|x| x.as_str())
                    .unwrap_or("vanilla_plus")
            ) == "enhanced_plus") as u8,
        light_shadows(
            v.get("light_shadows")
                .and_then(|x| x.as_str())
                .unwrap_or("off")
        ),
        b("cloud_shadows", false),
        b("light_glow", false)
    ));
    text.push_str(&format!(
        "rain_quality={}\n",
        rain_quality(
            v.get("rain_quality")
                .and_then(|x| x.as_str())
                .unwrap_or("high")
        )
    ));
    text.push_str(&format!(
        "vehicle_smoke={}\n",
        vehicle_smoke(
            v.get("vehicle_smoke")
                .and_then(|x| x.as_str())
                .unwrap_or("all")
        )
    ));
    text.push_str(&format!(
        "puddles={}\n",
        puddles(v.get("puddles").and_then(|x| x.as_str()).unwrap_or("high"))
    ));
    text.push_str(&format!(
        "road_glare={}\ntraffic_headlights={}\n",
        f("road_glare", 1.0).clamp(0.0, 2.0),
        f("traffic_headlights", 1.0).clamp(0.0, 2.0)
    ));
    text.push_str(&format!(
        "mirror_refresh={}\n",
        mirror_refresh(
            v.get("mirror_refresh")
                .and_then(|x| x.as_str())
                .unwrap_or("full")
        )
    ));
    text.push_str(&format!(
        "mirror_quality={}\n",
        mirror_quality(
            v.get("mirror_quality")
                .and_then(|x| x.as_str())
                .unwrap_or("standard")
        )
    ));
    text.push_str(&format!(
        "zoom_hold_amount={}\nzoom_hold_latch={}\n",
        f("zoom_hold_amount", 0.5).clamp(0.15, 1.0),
        b("zoom_hold_latch", false)
    ));
    text.push_str(&format!(
        "flashlight_cursor={}\n",
        b("flashlight_cursor", false)
    ));
    text.push_str(&format!("look_sens={}\nlook_smoothing_ms={}\nsteer_look_angle={}\nsteer_look_response={}\nhead_idle={}\nhead_idle_pace={}\ntime_sync={}\nmetar_sync={}\nmetar_station={}\n", f("look_sens", 1.0).clamp(0.1, 2.0), f("look_smoothing_ms", 0.0).clamp(0.0, 200.0), f("steer_look_angle", 30.0).clamp(0.0, 60.0), f("steer_look_response", 0.25).clamp(0.05, 1.0), f("head_idle", 0.0).clamp(0.0, 1.0), f("head_idle_pace", 1.0).clamp(0.5, 2.0), b("time_sync", false), b("metar_sync", false), v.get("metar_station").and_then(|x| x.as_str()).unwrap_or("").chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase()));
    text.push_str(&format!(
        "pad_steer_smooth={}\n",
        f("pad_steer_smooth", 120.0).clamp(0.0, 300.0)
    ));
    let triple_fov = f("triple_fov_deg", 0.0);
    text.push_str(&format!(
        "triple_hud_center={}\ntriple_fov_deg={}\n",
        b("triple_hud_center", true),
        if triple_fov < 20.0 {
            0.0
        } else {
            triple_fov.min(120.0)
        }
    ));
    text.push_str(&format!(
        "triple_screen={}\ntriple_span={}\n",
        b("triple_screen", false),
        b("triple_span", true)
    ));
    text.push_str(&format!(
        "triple_width_mm={}\ntriple_distance_mm={}\ntriple_bezel_mm={}\n",
        f("triple_width_mm", 600.0).clamp(200.0, 2000.0),
        f("triple_distance_mm", 650.0).clamp(200.0, 3000.0),
        f("triple_bezel_mm", 0.0).clamp(0.0, 100.0)
    ));
    text.push_str(&format!(
        "info_bar={}\nperf_overlay={}\n",
        b("info_bar", false),
        perf_overlay_mode(
            &v.get("perf_overlay")
                .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
                .unwrap_or_default()
        )
    ));
    text.push_str(&format!(
        "info_size={}\ntimetable_size={}\nperf_size={}\n",
        f("info_size", 1.0).clamp(0.6, 2.0),
        f("timetable_size", 1.0).clamp(0.6, 2.0),
        f("perf_size", 1.0).clamp(0.6, 2.0)
    ));
    text.push_str(&format!("windy_trees={}\n", b("windy_trees", true)));
    text.push_str(&format!("detailed_trees={}\n", b("detailed_trees", true)));
    text.push_str(&format!("road_snow={}\n", b("road_snow", false)));
    text.push_str(&format!("sim_thread={}\n", b("sim_thread", false)));
    text.push_str(&format!("mx_graphics={}\n", b("mx_graphics", false)));
    text.push_str(&format!(
        "mirror_hud_every_frame={}\n",
        b("mirror_hud_every_frame", false)
    ));
    text.push_str(&format!(
        "mirror_shadows={}\nmirror_particles={}\n",
        b("mirror_shadows", false),
        b("mirror_particles", false)
    ));
    text.push_str(&format!("mirror_hud={}\n", n("mirror_hud", 0).clamp(0, 7)));
    text.push_str(&format!(
        "standing_chance={}\n",
        n("standing_chance", 5).clamp(0, 100)
    ));
    text.push_str(&format!(
        "ambient={}\nvol_ambient={}\n",
        b("ambient", true),
        f("vol_ambient", 0.8).clamp(0.0, 1.0)
    ));
    text.push_str(&format!(
        "radio_speaker={}\nradio_hiss={}\nradio_dropouts={}\n",
        f("radio_speaker", 0.0).clamp(0.0, 1.0),
        f("radio_hiss", 0.0).clamp(0.0, 1.0),
        f("radio_dropouts", 0.0).clamp(0.0, 1.0)
    ));
    text.push_str(&format!(
        "ai_wait_timed_stops_only={}\n",
        b("ai_wait_timed_stops_only", false)
    ));
    text.push_str(&format!(
        "trip_summary={}\nfull_tank={}\n",
        b("trip_summary", true),
        b("full_tank", true)
    ));
    text.push_str(&format!("pax_bad_driving={}\n", b("pax_bad_driving", true)));
    text.push_str(&format!(
        "passenger_connections={}\npassenger_transfer_percent={}\n",
        b("passenger_connections", false),
        f("passenger_transfer_percent", 20.0).clamp(0.0, 100.0)
    ));
    text.push_str(&format!(
        "passenger_hailing={}\n",
        b("passenger_hailing", false)
    ));
    text.push_str(&format!(
        "passenger_full_bus_reaction={}\n",
        b("passenger_full_bus_reaction", false)
    ));
    text.push_str(&format!(
        "pax_short_rides_stand={}\n",
        b("pax_short_rides_stand", false)
    ));
    text.push_str(&format!("auto_announce={}\n", b("auto_announce", false)));
    text.push_str(&format!(
        "triple_left_angle_deg={}\ntriple_right_angle_deg={}\ntriple_eye_height_mm={}\n",
        f("triple_left_angle_deg", 45.0).clamp(0.0, 90.0),
        f("triple_right_angle_deg", 45.0).clamp(0.0, 90.0),
        f("triple_eye_height_mm", 0.0).clamp(-500.0, 500.0)
    ));
    text.push_str(&format!(
        "photo_mode={}\n",
        if v.get("photo_mode").and_then(|x| x.as_str()) == Some("advanced") {
            "advanced"
        } else {
            "simple"
        }
    ));
    text.push_str(&format!(
        "passenger_animation={}\n",
        if v.get("passenger_animation").and_then(|x| x.as_str()) == Some("enhanced") {
            "enhanced"
        } else {
            "original"
        }
    ));
    text.push_str(&format!(
        "character_models={}\n",
        if v.get("character_models").and_then(|x| x.as_str()) == Some("improved") {
            "improved"
        } else {
            "original"
        }
    ));
    text.push_str(&format!(
        "units={}\n",
        units_setting(v.get("units").and_then(|x| x.as_str()).unwrap_or("metric"))
    ));
    text.push_str(&format!(
        "head_tracking_source={}\ntobii_edges={}\nhead_tracking_smoothing={}\nhead_tracking_dead_zone={}\n",
        head_tracking_source(v.get("head_tracking_source").and_then(|x| x.as_str()).unwrap_or("auto")),
        b("tobii_edges", false),
        head_tracking_value("head_tracking_smoothing", v.get("head_tracking_smoothing").and_then(|x| x.as_f64())),
        head_tracking_value("head_tracking_dead_zone", v.get("head_tracking_dead_zone").and_then(|x| x.as_f64())),
    ));
    text.push_str(&format!(
        "upscaler={}\nupscale_quality={}\nframe_generation={}\nfsr_quality={}\nfsr_sharpness={}\n",
        upscaler_word(v.get("upscaler").and_then(|x| x.as_str()).unwrap_or("")),
        omsi_cfg::upscaler_files::quality_word(
            v.get("upscale_quality")
                .and_then(|x| x.as_str())
                .unwrap_or("")
        ),
        omsi_cfg::upscaler_files::frame_generation_word(
            v.get("frame_generation")
                .and_then(|x| x.as_str())
                .unwrap_or("")
        ),
        match v
            .get("fsr_quality")
            .and_then(|x| x.as_str())
            .unwrap_or("quality")
        {
            "ultra" => "ultra",
            "balanced" => "balanced",
            "performance" => "performance",
            "custom" => "custom",
            "auto" => "auto",
            _ => "quality",
        },
        f("fsr_sharpness", 0.8).clamp(0.0, 1.0),
    ));
    let written: Vec<String> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, _)| k.trim().to_ascii_lowercase())
        .collect();
    for line in old.unwrap_or("").lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
            continue;
        }
        match t.split_once('=') {
            Some((k, _))
                if !written.contains(&setting_key(k))
                    && !RETIRED_SETTINGS.contains(&setting_key(k).as_str()) =>
            {
                text.push_str(t);
                text.push('\n');
            }
            _ => {}
        }
    }
    text
}

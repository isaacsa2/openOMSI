//! Shared settings reader for the desktop and mobile launcher.

use super::*;

pub fn settings_from_text(text: Option<&str>) -> Value {
    let mut v = json!({ "msaa": 4, "anisotropy": 8, "ssao": true, "shadows": true, "shadow_size": 2048, "navigator": true, "ui_opacity": 0.85, "navigator_corner": "bottom-left", "boarding": "auto", "detail_textures": true, "exact_fare": true, "enhanced": false, "graphics": "vanilla_plus", "fullscreen": false, "vsync": true, "volume": 0.6, "drive_keys": "simple", "render_scale": "auto", "view_distance": "auto", "language": "ENG", "texture_memory": 0, "texture_compression": true, "gpu_texture_compression": true, "chat": true, "tooltips": true, "name_tags": true, "show_fps": false, "clouds": true, "cloud_quality": "high", "rain_quality": "high", "pax_density": 1.0, "vol_ai": 1.0, "vol_scenery": 1.0, "ambient": true, "vol_ambient": 0.8, "mirror_size": 256, "doppler": true, "driver": true, "max_fps": 0, "min_obj_size": 0.013, "max_obj_dist": "auto" });
    v["update_channel"] = json!("stable");
    v["touch_door_mode"] = json!("standard");
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
    v["discord_status"] = json!(true);
    // iPhone, iPad, Mac: the cross-platform launcher (this one, drawn by the game) instead of the
    // system's own (SwiftUI); elsewhere there is only this one
    v["cross_platform_launcher"] = json!(false);
    v["discord_app_id"] = json!("");
    // positional voice through GreenTeaSpeak's openOMSI plugin in multiplayer
    v["voice_chat"] = json!(true);
    // the launcher gives the graphics card up while a game runs (off: it stays drawn)
    v["launcher_rest"] = json!(true);
    // the window's size in pixels, "auto" to fit the screen (#904)
    v["resolution"] = json!("auto");
    // the game's information bar along the top, as the last session left it (#1164)
    v["info_bar"] = json!(false);
    v["perf_overlay"] = json!("off");
    // Human posing; the original OMSI behavior remains the default.
    v["passenger_animation"] = json!("enhanced");
    v["photo_mode"] = json!("simple");
    // the people as drawn: the content's own, or the Rocketbox avatars (this computer only)
    v["character_models"] = json!("original");
    v["units"] = json!("metric");
    // the trees' foliage bends and sways in the weather's wind
    v["windy_trees"] = json!(true);
    v["mirror_hud_every_frame"] = json!(false);
    // an early timetable bus waits only at the stops the timetable times (off: at every
    // stop, as in OMSI; #1773)
    v["ai_wait_timed_stops_only"] = json!(false);
    // OMSI's own options
    for (k, d) in [("maintenance", json!(0)), ("ai_unsched_factor", json!(100)), ("standing_chance", json!(5)), ("ai_max_scheduled", json!(0)), ("ai_max_parked", json!(0)), ("ai_max_humans", json!(200)), ("use_real_time", json!(false)), ("use_real_date", json!(false)), ("use_real_year", json!(false)), ("collision_vehicles", json!(true)), ("collision_objects", json!(true)), ("collision_pedestrians", json!(true)), ("head_movement", json!(true)), ("driverview_smooth", json!(true)), ("hands_in_cab", json!(false)), ("alt_view", json!(true)), ("precision_zoom", json!(false))] {
        v[k] = d;
    }
    // openOMSI's own: what passengers say, OMSI's route arrows, getting up from the seat
    for (k, d) in [("pax_voices", json!("all")), ("nav_arrows", json!(false)), ("nav_ai", json!(true)), ("nav_surface", json!(true)), ("nav_light", json!(false)), ("get_up", json!(false)), ("time_speed", json!("1")), ("time_sync", json!(false)), ("metar_sync", json!(false)), ("metar_station", json!("")), ("machine_translation", json!(false)), ("shadow_casters", json!("all")), ("shadow_blobs", json!(true)), ("reflections", json!(true)), ("mouse_sens", json!(1.0)), ("graphics_api", json!("auto")), ("ctrl_off", json!("")), ("steering_linear", json!(false)), ("old_steering", json!(false)), ("red_steer_spd", json!(false)), ("ff_invert", json!(false)), ("ff_enabled", json!(true)), ("brake_hold", json!(true)), ("auto_clutch", json!(true)), ("momentary_gears", json!(false)), ("auto_shift", json!(false)), ("wheel_range", json!(900.0)), ("wheel_lock", json!(0.0)), ("fov", json!(0.0)), ("camera_collision", json!(true)), ("right_stick_look", json!(true)), ("steer_look", json!(false)), ("pedal_throttle", json!(1.0)), ("pedal_brake", json!(1.0)), ("seat_x", json!(0.0)), ("seat_y", json!(0.0)), ("seat_z", json!(0.0)), ("seat_pitch_deg", json!(0.0)), ("head_tracking", json!(false)), ("head_tracking_yaw_sens", json!(100.0)), ("head_tracking_pitch_sens", json!(100.0)), ("head_tracking_roll_sens", json!(100.0)), ("head_tracking_x_sens", json!(100.0)), ("head_tracking_y_sens", json!(100.0)), ("head_tracking_z_sens", json!(100.0)), ("head_tracking_invert_yaw", json!(false)), ("head_tracking_invert_pitch", json!(false)), ("head_tracking_invert_roll", json!(false)), ("head_tracking_invert_x", json!(false)), ("head_tracking_invert_y", json!(false)), ("head_tracking_invert_z", json!(false)), ("led_glow", json!(6)), ("led_mips", json!(1.3)), ("night_brightness", json!(0.0)), ("ui_scale", json!(1.0)), ("ui_scale_window", json!(true)), ("chat_size", json!(1.0)), ("notes", json!(true)), ("mouse_steering", json!(false)), ("mouse_right_off", json!(false)), ("mouse_smooth", json!(true)), ("mouse_hold", json!(true)), ("mouse_pedals", json!(true)), ("key_pedal_speed", json!(1.0)), ("blinker_cancel", json!(true)), ("ff_road_vib", json!(1.0)), ("ff_engine_vib", json!(1.0)), ("ff_fade", json!(0.28))] {
        v[k] = d;
    }
    v["steer_look_angle"] = json!(30.0);
    v["look_sens"] = json!(1.0);
    v["look_smoothing_ms"] = json!(0.0);
    v["pad_steer_smooth"] = json!(120.0);
    v["pad_steer_linear"] = json!(false);
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
    for (k, d) in [("update_check", json!(true)), ("update_auto", json!(false)), ("update_notify", json!(true)), ("presence", json!(true))] {
        v[k] = d;
    }
    let Some(t) = text else { return v };
    let mut version = 0;
    let mut graphics: Option<&str> = None;
    for line in t.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, val)) = line.split_once('=') else { continue };
        let (k, val) = (setting_key(k), val.trim());
        let b = |x: &str| matches!(x.to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes");
        match k.as_str() {
            "anisotropy" => v[&k] = json!(val.parse::<i64>().unwrap_or(8).clamp(1, 16)),
            "msaa" | "shadow_size" => v[&k] = json!(val.parse::<i64>().unwrap_or(0)),
            "ui_opacity" | "volume" | "vol_ai" | "vol_scenery" | "vol_ambient" | "min_obj_size" | "radio_speaker" | "radio_hiss" | "radio_dropouts" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.0)),
            "pax_density" => v[&k] = json!(val.trim_end_matches('%').parse::<f64>().map(|x| if x > 5.0 { x / 100.0 } else { x }).unwrap_or(1.0)),
            "triple_fov_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) }).unwrap_or(0.0)),
            "triple_width_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(200.0, 2000.0)).unwrap_or(600.0)),
            "triple_distance_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(200.0, 3000.0)).unwrap_or(650.0)),
            "triple_bezel_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 100.0)).unwrap_or(0.0)),
            "triple_left_angle_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 90.0)).unwrap_or(45.0)),
            "triple_right_angle_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 90.0)).unwrap_or(45.0)),
            "triple_eye_height_mm" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(-500.0, 500.0)).unwrap_or(0.0)),
            "vr_scale" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.5, 1.0)).unwrap_or(0.65)),
            "vr_head_smoothing_ms" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 30.0) as i64).unwrap_or(0)),
            "vr_mirror_rate" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(-1.0, 360.0) as i64).unwrap_or(16)),
            "mirror_size" => v[&k] = json!(val.parse::<i64>().map(|x| if x == 0 { 0 } else { x.clamp(64, 2048) }).unwrap_or(256)),
            "mirror_refresh" => v[&k] = json!(mirror_refresh(val)),
            "mirror_quality" => v[&k] = json!(mirror_quality(val)),
            "cloud_quality" => v[&k] = json!(cloud_quality(val)),
            "rain_quality" => v[&k] = json!(rain_quality(val)),
            "gpu_texture_compression" => v[&k] = json!(!matches!(val.trim().to_ascii_lowercase().as_str(), "0" | "off" | "no" | "false" | "disabled")),
            "max_fps" => v[&k] = json!(val.parse::<f64>().map(|x| x as i64).unwrap_or(0)),
            "max_obj_dist" => v[&k] = if val.eq_ignore_ascii_case("auto") { json!("auto") } else { json!(val.parse::<f64>().map(|m| (m.round() as i64).to_string()).unwrap_or_else(|_| "auto".into())) },
            "ssao" | "shadows" | "shadow_blobs" | "navigator" | "enhanced" | "triple_screen" | "triple_span" | "triple_hud_center" | "vr" | "vr_desktop_mirror" | "fullscreen" | "vsync" | "exact_fare" | "detail_textures" | "texture_compression" | "chat" | "tooltips" | "name_tags" | "show_fps" | "clouds" | "doppler" | "driver" | "use_real_time" | "use_real_date" | "use_real_year" | "collision_vehicles" | "collision_objects" | "collision_pedestrians" | "head_movement" | "driverview_smooth" | "hands_in_cab" | "alt_view" | "precision_zoom" => v[&k] = json!(b(val)),
            "maintenance" | "ai_unsched_factor" | "ai_max_scheduled" | "standing_chance" => v[&k] = json!(val.trim_end_matches('%').parse::<f64>().map(|x| x.max(0.0) as i64).unwrap_or(0)),
            // (-1: no parked cars at all, #864)
            "ai_max_parked" => v[&k] = json!(val.parse::<f64>().map(|x| x.max(-1.0) as i64).unwrap_or(0)),
            "ai_max_humans" => v[&k] = json!(val.parse::<f64>().map(|x| x.max(1.0) as i64).unwrap_or(200)),
            "drive_keys" | "navigator_corner" | "boarding" | "render_scale" | "pax_voices" => v[&k] = json!(val),
            "touch_door_mode" => v[&k] = json!(if val.eq_ignore_ascii_case("direct") { "direct" } else { "standard" }),
            "photo_mode" => v[&k] = json!(if val.eq_ignore_ascii_case("advanced") { "advanced" } else { "simple" }),
            "passenger_animation" => v[&k] = json!(if val.eq_ignore_ascii_case("enhanced") { "enhanced" } else { "original" }),
            "character_models" => v[&k] = json!(if val.eq_ignore_ascii_case("improved") { "improved" } else { "original" }),
            "units" => v[&k] = json!(units_setting(val)),
            "ctrl_off" => v[&k] = json!(val),
            "metar_station" => v[&k] = json!(val.chars().filter(|c| c.is_ascii_alphabetic()).take(4).collect::<String>().to_ascii_uppercase()),
            "discord_app_id" => v[&k] = json!(val),
            "resolution" | "window_size" => v["resolution"] = json!(resolution_text(val)),
            "graphics_api" => v[&k] = json!(match val.to_ascii_lowercase().as_str() { "vulkan" => "vulkan", "dx12" => "dx12", "gl" => "gl", "angle" => "angle", _ => "auto" }),
            "shadow_casters" => v[&k] = json!(if val.eq_ignore_ascii_case("omsi") { "omsi" } else { "all" }),
            "ctrl_deadzone" => v[&k] = json!(val.parse::<f64>().unwrap_or(0.0).clamp(0.0, 0.3)),
            "mouse_sens" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.1, 3.0)),
            "key_pedal_speed" => v[&k] = json!(val.parse::<f64>().unwrap_or(1.0).clamp(0.25, 2.0)),
            "mouse_pedal_strength" => {
                v[&k] = json!(
                    val.parse::<f64>()
                        .ok()
                        .filter(|x| x.is_finite())
                        .unwrap_or(1.0)
                        .clamp(0.25, 4.0)
                )
            }
            "ui_scale" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(1.0).clamp(0.5, 2.0)),
            "chat_size" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(1.0).clamp(0.5, 3.0)),
            "wheel_range" => v[&k] = json!(val.parse::<f64>().unwrap_or(900.0).clamp(90.0, 2880.0)),
            "wheel_lock" => v[&k] = json!(val.parse::<f64>().map(|x| if x < 45.0 { 0.0 } else { x.min(2880.0) }).unwrap_or(0.0)),
            "fov" => v[&k] = json!(val.parse::<f64>().map(|x| if x < 20.0 { 0.0 } else { x.min(120.0) }).unwrap_or(0.0)),
            "pad_steer_speed" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.8, 5.0)).unwrap_or(2.0)),
            "pad_deadzone" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 0.4)).unwrap_or(0.08)),
            "pad_type" => v[&k] = json!(match val.trim().to_ascii_lowercase().as_str() { "xbox" => "xbox", "ps4" => "ps4", "ps5" => "ps5", _ => "auto" }),
            "camera_collision" | "right_stick_look" | "pad_steer_linear" | "pad_buttons" | "arrows_switch_cams" | "steer_look" | "head_tracking" | "head_tracking_invert_yaw" | "head_tracking_invert_pitch" | "head_tracking_invert_roll" | "head_tracking_invert_x" | "head_tracking_invert_y" | "head_tracking_invert_z" | "discord_status" | "voice_chat" | "launcher_rest" | "cross_platform_launcher" => v[&k] = json!(b(val)),
            // (how much of the mip chain an LED panel is held at, 0..4; a file from before
            // it was a number says 1 or 0)
            "led_mips" => v[&k] = json!(val.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 4.0)).unwrap_or(1.3)),
            "led_glow" => v[&k] = json!(val.parse::<i64>().map(|x| x.clamp(0, 15)).unwrap_or(6)),
            "night_brightness" => v[&k] = json!(val.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 3.0)).unwrap_or(0.0)),
            "look_sens" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.1, 2.0)).unwrap_or(1.0)),
            "look_smoothing_ms" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 200.0)).unwrap_or(0.0)),
            "pad_steer_smooth" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 300.0)).unwrap_or(120.0)),
            "steer_look_angle" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 60.0)).unwrap_or(30.0)),
            "steer_look_response" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.05, 1.0)).unwrap_or(0.25)),
            "head_idle" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 1.0)).unwrap_or(0.0)),
            "head_idle_pace" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.5, 2.0)).unwrap_or(1.0)),
            "pedal_throttle" | "pedal_brake" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.25, 4.0)).unwrap_or(1.0)),
            "ff_road_vib" | "ff_engine_vib" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.0, 4.0)).unwrap_or(1.0)),
            "ff_fade" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(0.0, 1.5)).unwrap_or(0.28)),
            "seat_x" | "seat_y" | "seat_z" => v[&k] = json!(val.parse::<f64>().map(|x| x.clamp(-1.5, 1.5)).unwrap_or(0.0)),
            "seat_pitch_deg" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(0.0).clamp(-45.0, 45.0)),
            "head_tracking_yaw_sens" | "head_tracking_pitch_sens" | "head_tracking_roll_sens" | "head_tracking_x_sens" | "head_tracking_y_sens" | "head_tracking_z_sens" => v[&k] = json!(val.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 100.0)).unwrap_or(100.0)),
            "nav_arrows" | "nav_ai" | "nav_surface" | "nav_light" | "get_up" | "time_sync" | "metar_sync" | "ui_scale_window" | "notes" | "machine_translation" | "update_check" | "update_auto" | "update_notify" | "presence" | "reflections" | "steering_linear" | "old_steering" | "red_steer_spd" | "ff_invert" | "ff_enabled" | "brake_hold" | "auto_clutch" | "momentary_gears" | "auto_shift" | "mouse_steering" | "mouse_right_off" | "mouse_smooth" | "mouse_hold" | "mouse_pedals" | "blinker_cancel" => v[&k] = json!(b(val)),
            "update_channel" => v[&k] = json!(if val.eq_ignore_ascii_case("beta") { "beta" } else { "stable" }),
            "info_bar" => v[&k] = json!(b(val)),
            "perf_overlay" => v[&k] = json!(perf_overlay_mode(val)),
            "windy_trees" | "mirror_hud_every_frame" | "ai_wait_timed_stops_only" | "ambient" => v[&k] = json!(b(val)),
            "time_speed" => v[&k] = json!(val.trim_start_matches(['x', 'X']).parse::<f64>().map(|x| x.clamp(1.0, 30.0)).map(|x| if x.fract() == 0.0 { format!("{}", x as i64) } else { x.to_string() }).unwrap_or_else(|_| "1".into())),
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
            "texture_memory" => {
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
    let g = graphics.unwrap_or(if v["enhanced"] == json!(true) { "enhanced" } else { "vanilla_plus" });
    v["graphics"] = json!(g);
    v["enhanced"] = json!(g == "enhanced" || g == "enhanced_plus");
    // before version 2 the launcher wrote its old default `boarding=pay` for everybody
    // (passengers then waited at the cash desk for the driver): the game reads that as auto
    if version < 2 && v["boarding"] == "pay" {
        v["boarding"] = json!("auto");
    }
    // before version 3 `passenger_animation=original` was everybody's default, not a choice:
    // the people's own (procedural) poses are the default now
    if version < 3 && v["passenger_animation"] == "original" {
        v["passenger_animation"] = json!("enhanced");
    }
    v
}

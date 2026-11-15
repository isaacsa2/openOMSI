//! Recognized functional mesh tags mapped to native animation/material directives.
use crate::{
    ini,
    settings::{number, position},
    vehicle::Inspection,
    vehicle_physics::Physics,
    Ini,
};
use anyhow::Result;
use glam::{Quat, Vec3};

pub(crate) fn wiper_frame(name: &str) -> Option<(usize, usize)> {
    for i in 1..=2 {
        if let Some(s) = name.split(&format!("_wipers{i}_")).nth(1) {
            let digits = s.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                if let Ok(n) = s[..digits].parse::<usize>() {
                    if n <= 255 {
                        return Some((i - 1, n));
                    }
                }
            }
        }
    }
    None
}
pub(crate) fn append(
    out: &mut String,
    name: &str,
    physics: &Physics,
    info: &Inspection,
    models: &Ini,
    materials: &[omsi_o3d::Material],
) -> Result<()> {
    let name = name.trim().to_ascii_lowercase();
    for (tag, axle, side) in [
        ("fl", 0, "L"),
        ("fr", 0, "R"),
        ("rl", 1, "L"),
        ("rr", 1, "R"),
        ("rl2", 2, "L"),
        ("rr2", 2, "R"),
        ("rl3", 3, "L"),
        ("rr3", 3, "R"),
    ] {
        let spin = name.contains(&format!("_wheel_{tag}_"));
        if !spin
            && !name.contains(&format!("_wheel_caliper_{tag}_"))
            && !name.contains(&format!("_wheel_suspension_{tag}_"))
        {
            continue;
        }
        if let Some(a) = physics.axles.get(axle) {
            let p = if side == "L" { a.left } else { a.right };
            if spin {
                animation(
                    out,
                    p,
                    Vec3::X,
                    &format!("Wheel_Rotation_{axle}_{side}"),
                    57.29578,
                    0.,
                );
            }
            if axle == 0 {
                animation(
                    out,
                    p,
                    Vec3::Z,
                    &format!("Axle_Steering_{axle}_{side}"),
                    57.29578,
                    0.,
                );
            }
            translation(out, &format!("Axle_Suspension_{axle}_{side}"));
        }
    }
    for (i, door) in info.doors.iter().enumerate() {
        let side = if door.name.starts_with("right") {
            "r"
        } else {
            "l"
        };
        let n = door
            .name
            .trim_start_matches("rightdoor")
            .trim_start_matches("leftdoor");
        let file = info
            .components
            .iter()
            .find(|c| c.role == door.name)
            .unwrap();
        let d = ini(&file.file)?;
        for leaf in 1..=2 {
            if name.contains(&format!("_{side}door_{n}x{leaf}_")) {
                let pivot = format!("anim1_door{leaf}");
                anyhow::ensure!(d.contains_key(&pivot), "missing {pivot} for {}", door.name);
                animation(
                    out,
                    position(&d, &pivot)?,
                    Vec3::Z,
                    &format!("oi_door_{i}"),
                    number(&d, &door.name, &format!("anim_rotate_{leaf}"), 0.)?,
                    0.,
                );
            }
        }
        for state in ["on", "off"] {
            if name.contains(&format!("_{side}doorbutton{n}_{state}_")) {
                visible(
                    out,
                    &format!("oi_door_target_{i}"),
                    if state == "on" { 1. } else { 0. },
                );
                out.push_str(&format!("[mouseevent]\noi_door_button_{i}\n"));
            }
        }
    }
    for (tag, section, var, default) in [
        ("_steering_wheel_", "steering_wheel", "oi_steer", 450.),
        ("_speed_", "speed", "Velocity", 120.),
        ("_rpm_", "rpm", "engine_n", 3000.),
    ] {
        if name.contains(tag) && models.contains_key(section) {
            let x = number(models, section, "rotx", 0.)?.to_radians();
            let y = number(models, section, "roty", 0.)?.to_radians();
            let z = number(models, section, "rotz", 0.)?.to_radians();
            let q = Quat::from_euler(glam::EulerRot::YXZ, y, x, z);
            let axis = q * if section == "steering_wheel" {
                Vec3::Y
            } else {
                Vec3::Z
            };
            let axis = Vec3::new(axis.x, axis.z, axis.y);
            let factor = if section == "steering_wheel" {
                number(models, section, "totalturningangle", default)?
            } else {
                number(models, section, "maxangle", 240.)?
                    / number(models, section, "maxvalue", default)?.max(1.)
            };
            animation(out, position(models, section)?, axis, var, factor, 0.);
        }
    }
    if let Some((i, n)) = wiper_frame(&name) {
        visible(out, &format!("oi_wiper_{}", i + 1), n as f32);
    }
    // Wet overlays are optional geometry; keep them out until native rain-mask mapping exists.
    if name.contains("_windows_wet_") {
        visible(out, "oi_wet_overlay", 1.);
    }
    for (tag, var) in [
        ("_cockpit_handbrake_", "parking_brake"),
        ("_cockpit_low_beam_", "lights_abblend"),
        ("_cockpit_high_beam_", "lights_fern"),
        ("_cockpit_left_blinker_", "oi_blink_left"),
        ("_cockpit_right_blinker_", "oi_blink_right"),
        ("_cockpit_stop_request_", "oi_stop_request"),
        ("_tex_low_beam_", "lights_abblend"),
        ("_tex_high_beam_", "lights_fern"),
        ("_tex_left_blinker_", "oi_blink_left"),
        ("_tex_right_blinker_", "oi_blink_right"),
        ("_tex_brake_", "oi_brake_lamp"),
        ("_tex_reverse_", "oi_reverse"),
        ("_tex_external_lights_", "lights_stand"),
    ] {
        if name.contains(tag) {
            visible(out, var, 1.);
        }
    }
    for (tag, var, trigger) in [
        ("parking_brake", "parking_brake", "parking_brake_toggle"),
        ("btlowbeam", "lights_abblend", "kw_scheinwerfer_toggle"),
        ("bthighbeam", "lights_fern", "kw_fernlicht_toggle"),
        ("bthazardlight", "lights_warn", "blinker_warn_toggle"),
        ("btwipers1", "oi_wiper_on", "kw_wipermode_up"),
        ("btwipers2", "oi_wiper_on", "kw_wipermode_up"),
        ("btlight1", "lights_interior", "cp_licht_unterdeck_toggle"),
        ("btlight2", "lights_interior", "cp_licht_unterdeck_toggle"),
        ("btlight3", "lights_interior", "cp_licht_unterdeck_toggle"),
    ] {
        for state in ["on", "off"] {
            if name.contains(&format!("_{tag}_{state}_")) {
                visible(out, var, if state == "on" { 1. } else { 0. });
                out.push_str(&format!("[mouseevent]\n{trigger}\n"));
            }
        }
    }
    let mut view = 0;
    if name.contains("_icam_") || name.contains("_pcam_") {
        view |= 2;
    }
    if name.contains("_ecam_") {
        view |= 5;
    }
    if view != 0 {
        out.push_str(&format!("[viewpoint]\n{view}\n"));
    }
    controls(out, &name);
    for (tag, index) in [
        ("_display_front_", 0),
        ("_old_display_destination_", 0),
        ("_plate_side_", 0),
        ("_display_back_", 1),
        ("_old_display_route_", 1),
        ("_plate_route_small_", 1),
    ] {
        if name.contains(tag) {
            for (i, _) in materials.iter().enumerate() {
                select_material(out, materials, i);
                out.push_str(&format!("[useTextTexture]\n{index}\n[matl_alpha]\n2\n[matl_noZwrite]\n[matl_allcolor]\n0\n0\n0\n1\n0\n0\n0\n0\n0\n0\n1\n0.7\n0\n0\n"));
            }
        }
    }
    Ok(())
}
fn visible(out: &mut String, var: &str, v: f32) {
    out.push_str(&format!("[visible]\n{var}\n{v}\n"));
}
fn translation(out: &mut String, var: &str) {
    out.push_str(&format!(
        "[newanim]\norigin_rot_y\n90\nanim_trans\n{var}\n1\n"
    ));
}
fn animation(out: &mut String, p: [f32; 3], axis: Vec3, var: &str, f: f32, speed: f32) {
    let elevation = axis.z.atan2(axis.truncate().length()).to_degrees();
    let azimuth = axis.y.atan2(axis.x).to_degrees();
    out.push_str(&format!("[newanim]\norigin_trans\n{}\n{}\n{}\norigin_rot_z\n{}\norigin_rot_y\n{elevation}\nanim_rot\n{var}\n{f}\n",p[0],p[1],p[2],-azimuth));
    if speed > 0. {
        out.push_str(&format!("maxspeed\n{speed}\n"));
    }
}
pub(crate) fn select_material(out: &mut String, materials: &[omsi_o3d::Material], index: usize) {
    let m = &materials[index];
    let n = materials[..index]
        .iter()
        .filter(|v| v.texture.eq_ignore_ascii_case(&m.texture))
        .count();
    out.push_str(&format!("[matl]\n{}\n{n}\n", m.texture));
}

fn controls(out: &mut String, name: &str) {
    for (tag, value) in [
        ("_key_off_", 0.),
        ("_key_stage1_", 1.),
        ("_key_stage2_", 2.),
    ] {
        if name.contains(tag) {
            visible(out, "oi_ignition", value);
            out.push_str("[mouseevent]\nkw_m_enginestart\n");
        }
    }
    for (tag, var, trigger) in [
        ("d", "oi_selected_d", "automatic_D"),
        ("n", "oi_selected_n", "automatic_N"),
        ("r", "oi_selected_r", "automatic_R"),
    ] {
        for state in ["on", "off"] {
            if name.contains(&format!("_autogear_{tag}_{state}_")) {
                visible(out, var, if state == "on" { 1. } else { 0. });
                out.push_str(&format!("[mouseevent]\n{trigger}\n"));
            }
        }
    }
    for i in 1..=16 {
        if name.contains(&format!("_manualgear{i}_")) {
            visible(out, "oi_shown_gear", i as f32);
            out.push_str(&format!("[mouseevent]\nkw_s_{i}\n"));
        }
    }
    if name.contains("_manualgearn_") {
        visible(out, "oi_shown_gear", 0.);
        out.push_str("[mouseevent]\nautomatic_N\n");
    }
    if name.contains("_manualgearr_") {
        visible(out, "oi_shown_gear", -1.);
        out.push_str("[mouseevent]\nautomatic_R\n");
    }
    for (tag, var) in [
        ("_cockpit_battery_", "oi_battery_warning"),
        ("_cockpit_panel_", "lights_abblend"),
        ("_tex_electric_", "elec_busbar_main"),
        ("_cockpit_blinkers_combined_", "oi_blink_any"),
        ("_tex_stop_request_", "oi_stop_request"),
    ] {
        if name.contains(tag) {
            visible(out, var, 1.);
        }
    }
}

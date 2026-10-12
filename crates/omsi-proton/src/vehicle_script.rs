//! Generated native scripts, independent of any original vehicle script implementation.
use crate::{vehicle::Inspection, vehicle_cabin::Cabin, vehicle_physics::Physics};

pub(crate) fn generate(
    p: &Physics,
    info: &Inspection,
    cabin: &Cabin,
    wipers: [usize; 2],
) -> (String, String, String) {
    let mut vars = vec![
        "M_Wheel",
        "Brakeforce",
        "Throttle",
        "Brake",
        "Clutch",
        "Velocity",
        "Velocity_Ground",
        "n_Wheel",
        "engine_n",
        "engine_on",
        "engine_tank_content",
        "engine_tank_capacity",
        "engine_temp",
        "elec_busbar_main",
        "parking_brake",
        "oi_gear",
        "oi_selector",
        "oi_ratio",
        "oi_throttle",
        "oi_rpm_norm",
        "oi_shift_timer",
        "lights_stand",
        "lights_abblend",
        "lights_fern",
        "lights_interior",
        "lights_blinker_l",
        "lights_blinker_r",
        "lights_warn",
        "oi_blink_phase",
        "oi_blink_left",
        "oi_blink_right",
        "oi_brake",
        "oi_brake_lamp",
        "oi_reverse",
        "oi_steer",
        "oi_wiper_on",
        "oi_wiper_phase",
        "oi_wiper_1",
        "oi_wiper_2",
        "horn",
        "oi_stop_request",
        "IBIS_LinieKurs",
        "IBIS_TerminusIndex",
        "IBIS_TerminusCode",
        "target_index_int",
        "AI_target_index",
        "oi_ignition",
        "oi_selected_d",
        "oi_selected_n",
        "oi_selected_r",
        "oi_shown_gear",
        "oi_battery_warning",
        "oi_blink_any",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    let mut init="1 (S.L.elec_busbar_main) (S.L.parking_brake) 0 (S.L.engine_on) (S.L.oi_selector) 1 (S.L.oi_gear) 250 (S.L.engine_tank_capacity) (S.L.engine_tank_content) 80 (S.L.engine_temp)".to_string();
    init.push_str(" -1 (S.L.target_index_int)");
    let mut frame=format!("(L.L.n_Wheel) abs (L.L.oi_ratio) abs * {} * {} max (S.L.engine_n)\n(L.L.engine_on) ! {{if}} 0 (S.L.engine_n) {{endif}}\n(L.L.engine_n) {} / 0 max 1 min (S.L.oi_rpm_norm)\n(L.L.Throttle) 0 max 1 min (S.L.oi_throttle)\n(L.L.oi_shift_timer) (L.S.Timegap) - 0 max (S.L.oi_shift_timer)\n",p.differential,p.idle,p.max_rpm);
    // Hysteresis and a cooldown avoid jumping through all ratios in one frame.
    frame.push_str(&format!("(L.L.oi_selector) 1 = (L.L.oi_shift_timer) 0 = && {{if}}\n(L.L.engine_n) {} > (L.L.oi_gear) {} < && {{if}} (L.L.oi_gear) 1 + (S.L.oi_gear) 0.4 (S.L.oi_shift_timer) {{else}}\n(L.L.engine_n) {} < (L.L.oi_gear) 1 > && {{if}} (L.L.oi_gear) 1 - (S.L.oi_gear) 0.4 (S.L.oi_shift_timer) {{endif}} {{endif}} {{endif}}\n0 (S.L.oi_ratio)\n",p.shift_up,p.ratios.len(),p.shift_down));
    for (i, r) in p.ratios.iter().enumerate() {
        frame.push_str(&format!(
            "(L.L.oi_gear) {} = {{if}} {r} (S.L.oi_ratio) {{endif}}\n",
            i + 1
        ));
    }
    frame.push_str(&format!("(L.L.oi_selector) 0 = {{if}} 0 (S.L.oi_ratio) {{endif}}\n(L.L.oi_selector) -1 = {{if}} {} (S.L.oi_ratio) {{endif}}\n(L.L.oi_throttle) {} * (L.L.oi_ratio) * {} * 0.9 * (L.L.engine_on) * (L.L.elec_busbar_main) * (L.L.parking_brake) ! * (S.L.M_Wheel)\n(L.L.engine_n) {} >= {{if}} 0 (S.L.M_Wheel) {{endif}}\n(L.L.Brake) 0 max 1 min (S.L.oi_brake)\n(L.L.oi_brake) 0.05 > (S.L.oi_brake_lamp)\n(L.L.oi_selector) -1 = (S.L.oi_reverse)\n(L.L.Axle_Steering_0_L) {} / -1 max 1 min (S.L.oi_steer)\n(L.L.oi_blink_phase) (L.S.Timegap) + 1 % (S.L.oi_blink_phase)\n(L.L.oi_blink_phase) 0.5 < (L.L.lights_blinker_l) (L.L.lights_warn) || && (L.L.elec_busbar_main) && (S.L.oi_blink_left)\n(L.L.oi_blink_phase) 0.5 < (L.L.lights_blinker_r) (L.L.lights_warn) || && (L.L.elec_busbar_main) && (S.L.oi_blink_right)\n",p.reverse,p.torque,p.differential,p.max_rpm,p.max_steer.to_radians()));
    frame.push_str("(L.L.elec_busbar_main) (L.L.engine_on) + (S.L.oi_ignition)\n(L.L.oi_selector) 1 = (S.L.oi_selected_d)\n(L.L.oi_selector) 0 = (S.L.oi_selected_n)\n(L.L.oi_selector) -1 = (S.L.oi_selected_r)\n(L.L.oi_selector) (S.L.oi_shown_gear) 1 = {if} (L.L.oi_gear) (S.L.oi_shown_gear) {endif}\n(L.L.elec_busbar_main) (L.L.engine_on) ! && (S.L.oi_battery_warning)\n(L.L.oi_blink_left) (L.L.oi_blink_right) || (S.L.oi_blink_any)\n");
    // M_Wheel is TOTAL wheel torque; brakes are N per wheel, not Nm.
    for (i, a) in p.axles.iter().enumerate() {
        for side in ["L", "R"] {
            frame.push_str(&format!(
                "(L.L.oi_brake) {} * (L.L.parking_brake) {} * + (S.L.Axle_Brakeforce_{i}_{side})\n",
                p.brake_torque / a.radius,
                if a.driven {
                    p.parking_torque / a.radius
                } else {
                    0.
                }
            ));
        }
    }
    for (i, _) in info.doors.iter().enumerate() {
        vars.extend([format!("oi_door_{i}"), format!("oi_door_target_{i}")]);
        init.push_str(&format!("\n0 (S.L.oi_door_{i}) (S.L.oi_door_target_{i})"));
        for (kind, indices) in [("Entry", &cabin.entries), ("Exit", &cabin.exits)] {
            for (index, _) in indices.iter().enumerate().filter(|(_, d)| **d == i) {
                frame.push_str(&format!(
                    "(L.L.PAX_{kind}{index}_Busy) {{if}} 1 (S.L.oi_door_target_{i}) {{endif}}\n"
                ));
            }
        }
        frame.push_str(&format!("(L.L.oi_door_target_{i}) (L.L.oi_door_{i}) - (L.S.Timegap) 1.25 * min (L.S.Timegap) -1.25 * max (L.L.oi_door_{i}) + 0 max 1 min (S.L.oi_door_{i})\n"));
        // Do not drive off while a door is open; actual animated state feeds boarding.
        frame.push_str(&format!(
            "(L.L.oi_door_{i}) 0.05 > {{if}} 0 (S.L.M_Wheel) {} (S.L.Brakeforce) {{endif}}\n",
            p.brake_torque / p.axles[0].radius
        ));
        for (kind, indices) in [("Entry", &cabin.entries), ("Exit", &cabin.exits)] {
            for (index, &physical) in indices.iter().enumerate().filter(|(_, d)| **d == i) {
                vars.extend([
                    format!("PAX_{kind}{index}_Open"),
                    format!("PAX_{kind}{index}_Busy"),
                    format!("PAX_{kind}{index}_Req"),
                ]);
                frame.push_str(&format!(
                    "(L.L.oi_door_{physical}) 0.95 > (S.L.PAX_{kind}{index}_Open)\n"
                ));
            }
        }
    }
    frame.push_str("(L.L.oi_wiper_on) {if} (L.L.oi_wiper_phase) (L.S.Timegap) + 1 % (S.L.oi_wiper_phase) {else} 0 (S.L.oi_wiper_phase) {endif}\n");
    for (i, n) in wipers.iter().enumerate() {
        if *n > 0 {
            frame.push_str(&format!(
                "(L.L.oi_wiper_phase) 0.5 - abs 2 * -1 * 1 + {} * trunc (S.L.oi_wiper_{})\n",
                n - 1,
                i + 1
            ));
        }
    }
    let mut script = format!("{{init}}\n{init}\n{{end}}\n{{frame}}\n{frame}\n{{end}}\n");
    for (names, var) in [
        (
            &["kw_m_enginestart", "kw_m_engine_startbutton"][..],
            "engine_on",
        ),
        (
            &["parking_brake_toggle", "parking_brake_mouse"],
            "parking_brake",
        ),
        (&["kw_scheinwerfer_toggle"], "lights_abblend"),
        (&["kw_standlicht_toggle"], "lights_stand"),
        (&["kw_fernlicht_toggle"], "lights_fern"),
        (
            &[
                "cp_licht_unterdeck_toggle",
                "cp_licht_untenrechts_toggle",
                "cp_licht_oberdeck_toggle",
                "cp_fahrerlicht_toggle",
            ],
            "lights_interior",
        ),
        (&["blinker_warn_toggle"], "lights_warn"),
        (
            &["kw_wipermode_up", "cp_wischer_intervall_toggle"],
            "oi_wiper_on",
        ),
        (&["kw_electricity_toggle"], "elec_busbar_main"),
    ] {
        for n in names {
            trigger(&mut script, n, &format!("(L.L.{var}) ! (S.L.{var})"));
        }
    }
    for (n, var, val) in [
        ("parking_brake_set", "parking_brake", 1),
        ("parking_brake_release", "parking_brake", 0),
        ("horn", "horn", 1),
        ("horn_off", "horn", 0),
        ("blinker_left_set", "lights_blinker_l", 1),
        ("blinker_right_set", "lights_blinker_r", 1),
    ] {
        trigger(&mut script, n, &format!("{val} (S.L.{var})"));
    }
    trigger(
        &mut script,
        "blinker_off",
        "0 (S.L.lights_blinker_l) (S.L.lights_blinker_r)",
    );
    for (names, selector) in [
        (
            &["automatic_D", "automatic_1", "automatic_2", "automatic_3"][..],
            1,
        ),
        (&["automatic_N", "kw_s_N"], 0),
        (&["automatic_R", "kw_s_R"], -1),
    ] {
        for n in names {
            trigger(&mut script,n,&format!("(L.L.Velocity_Ground) abs 0.5 < {{if}} {selector} (S.L.oi_selector) 1 (S.L.oi_gear) {{endif}}"));
        }
    }
    for g in 1..=p.ratios.len() {
        trigger(
            &mut script,
            &format!("kw_s_{g}"),
            &format!("1 (S.L.oi_selector) {g} (S.L.oi_gear)"),
        );
    }
    for i in 0..info.doors.len() {
        let toggle=format!("(L.L.Velocity_Ground) abs 0.5 < {{if}} (L.L.oi_door_target_{i}) ! (S.L.oi_door_target_{i}) {{endif}}");
        trigger(&mut script, &format!("oi_door_button_{i}"), &toggle);
        trigger(&mut script, &format!("bus_doorfront{i}"), &toggle);
        trigger(&mut script, &format!("bus_door_{}", i + 1), &toggle);
    }
    // Each rear door is independently clickable; this stock action closes them all.
    if info.doors.len() > 1 {
        let rear = (1..info.doors.len())
            .map(|i| format!("(L.L.oi_door_target_1) ! (S.L.oi_door_target_{i})\n"))
            .collect::<String>();
        // Store the first rear target last so all rear doors take the same state.
        let rear = rear.lines().rev().collect::<Vec<_>>().join("\n");
        trigger(
            &mut script,
            "bus_dooraft",
            &format!("(L.L.Velocity_Ground) abs 0.5 < {{if}} {rear} {{endif}}"),
        );
    }
    let close = (1..info.doors.len())
        .map(|i| format!("0 (S.L.oi_door_target_{i})\n"))
        .collect::<String>();
    trigger(&mut script, "bus_dooraftclose", &close);
    trigger(
        &mut script,
        "ai_scheduled_settarget",
        "(L.L.AI_target_index) (S.L.target_index_int)",
    );
    trigger(&mut script, "kw_wipermode_down", "0 (S.L.oi_wiper_on)");
    // All identifiers referenced by model/sound/physics must exist even without OMSI_ROOT.
    for a in 0..p.axles.len() {
        for side in ["L", "R"] {
            for prefix in [
                "Wheel_Rotation",
                "Axle_Steering",
                "Axle_Suspension",
                "Axle_Brakeforce",
            ] {
                vars.push(format!("{prefix}_{a}_{side}"));
            }
        }
    }
    vars.sort();
    vars.dedup();
    (script,vars.join("\n"),"ident\nnumber\nact_route\nact_busstop\nSetLineTo\nIBIS\nIBIS_cabindisplay\nIBIS_Linie\nIBIS_Ziel\nIBIS_terminus_name\nIBIS_Complex_Line\nIBIS_terminus_texture\n".into())
}
fn trigger(s: &mut String, n: &str, body: &str) {
    s.push_str(&format!("{{trigger:{n}}}\n{body}\n{{end}}\n"));
}

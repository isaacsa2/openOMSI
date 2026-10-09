//! Opt-in restoration of the earlier procedural human poses.
//! Keep the original OMSI animation as the default for compatibility and A/B testing.

use super::*;
use glam::{Affine3A, Vec3};
use omsi_sim::human::{Activity, Pose, PoseInput, SLOTS};
use omsi_sim::people::{pax::Task, BusId};

fn is_procedural_mode(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "procedural" | "enhanced" | "1")
}

pub(super) fn enabled() -> bool {
    // Read once per game session (PeopleView::new), not once per process: on Android
    // another game is started in the launcher's existing process.
    let mode = omsi_cfg::flags::OMSI_PAX_ANIMATION.live_var()
        .unwrap_or_else(|_| crate::settings::Settings::load().passenger_animation);
    is_procedural_mode(&mode)
}

/// Only riders assigned a standing place should hold a pole. Queued and walking
/// passengers must keep their hands free to use the doors and ticket machines.
fn grip_for_standing_rider(inside: bool, assigned_standing: bool, walking: bool, reaching: bool) -> f32 {
    if inside && assigned_standing && !walking && !reaching { 1.0 } else { 0.0 }
}

/// The previous foot-planted / IK pose system, driven by the current people's state.
/// This changes only how bodies are skinned, never boarding, movement or ticket logic.
pub(super) fn bones(p: &mut Person, dt: f32) -> [Affine3A; SLOTS] {
    let (origin, heading, frame) = match p.place {
        Place::Ground => (p.position, p.heading, 0),
        Place::Bus(bus, at) => {
            let frame = match bus {
                BusId::Player => 1,
                BusId::Ai(id) => id.wrapping_add(2),
            };
            (at.as_dvec3(), p.lheading, frame)
        }
    };
    let (activity, seat, reach, hold) = match &p.state {
        State::Pax(x) => {
            let kind = x.pax_state.round().clamp(0.0, 2.0) as u8;
            let activity = match kind {
                2 => Activity::Sit,
                1 => Activity::Walk,
                _ if x.reach => Activity::Pay,
                _ => Activity::Stand,
            };
            let seat = (kind == 2).then(|| Vec3::new(0.0, 0.0, x.seat_h));
            let reach = (x.reach && x.inside.is_some()).then(|| {
                let d = x.reach_at.as_dvec3() - x.pos;
                let (s, c) = x.yaw.sin_cos();
                Vec3::new((d.x * c - d.y * s) as f32, (d.x * s + d.y * c) as f32, d.z as f32)
            });
            let hold = grip_for_standing_rider(
                x.inside.is_some(),
                x.task == Task::SittingInBus && kind == 0,
                kind == 1,
                x.reach,
            );
            (activity, seat, reach, hold)
        }
        _ => (p.activity, None, None, 0.0),
    };
    let input = PoseInput {
        activity,
        origin,
        heading,
        frame,
        velocity: p.vel,
        seat,
        reach,
        hold,
        ..PoseInput::default()
    };
    let seed = p.id;
    let pose = p.procedural.get_or_insert_with(|| Pose::new(seed));
    pose.advance(&p.ty.rig, &input, dt);
    pose.bones(&p.ty.rig).bones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stationary_standing_riders_hold_the_rail() {
        assert_eq!(grip_for_standing_rider(true, true, false, false), 1.0);
        assert_eq!(grip_for_standing_rider(true, false, false, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, true, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, false, true), 0.0);
        assert_eq!(grip_for_standing_rider(false, true, false, false), 0.0);
    }

    #[test]
    fn enhanced_pose_requires_explicit_opt_in() {
        assert!(!is_procedural_mode(""));
        assert!(!is_procedural_mode("original"));
        assert!(!is_procedural_mode("0"));
        assert!(is_procedural_mode("procedural"));
        assert!(is_procedural_mode("enhanced"));
        assert!(is_procedural_mode("ENHANCED"));
        assert!(is_procedural_mode("1"));
    }
}

//! Headless authoritative interest and passenger-claim processing.
//!
//! Production TrafficSim and PeopleSim are driven by the connected clients'
//! actual locations, not by a nonexistent dedicated-server camera. Only the
//! host may process boarding claims; the transport acknowledges each claim
//! using the existing network protocol and ownership rules.

use glam::DVec3;
use omsi_net::{LanSession, Pose, Role};
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::ai_traffic::light_paths::box_outline;
use omsi_sim::ai_traffic::model::PlayerBox;
use omsi_sim::ai_traffic::viewer::Viewer;
use omsi_sim::people::PeopleSim;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InterestUpdate {
    pub visible_players: usize,
    pub boarding_claims: usize,
    pub granted: usize,
    pub denied: usize,
}

/// Ignore non-finite or clearly invalid client state before passing it to
/// pathfinding, collision or pedestrian visibility. Legacy clients with no
/// supplied vehicle box get the same conservative car/bus fallback as the
/// graphical host until the vehicle's exact model is available server-side.
fn vehicle_box(pose: &Pose) -> Option<(DVec3, PlayerBox)> {
    if !pose.has_vehicle()
        || !pose.x.is_finite()
        || !pose.y.is_finite()
        || !pose.z.is_finite()
        || !pose.heading.is_finite()
        || [pose.x, pose.y, pose.z].iter().any(|v| v.abs() > 1.0e8)
    {
        return None;
    }
    let position = DVec3::new(pose.x, pose.y, pose.z);
    let width = if pose.width.is_finite() && pose.width > 0.1 {
        pose.width.clamp(1.0, 5.0)
    } else {
        2.5
    };
    let length = if pose.length.is_finite() && pose.length > 0.1 {
        pose.length.clamp(2.5, 40.0)
    } else {
        11.0
    };
    let offset = if pose.box_offset.is_finite() {
        pose.box_offset.clamp(-20.0, 20.0)
    } else {
        0.0
    };
    let speed = if pose.speed_kmh.is_finite() {
        (pose.speed_kmh / 3.6).clamp(-30.0, 90.0)
    } else {
        0.0
    };
    let body = box_outline(
        position,
        f64::from(pose.heading),
        [width, length, 3.0, 0.0, offset, 1.5],
        speed,
    );
    Some((position, body))
}

/// Update both simulations before their physics tick and grant/deny passengers
/// atomically at the server. This does not serialize LAN world frames or
/// handle vehicle content: those remain separate, security-checked services.
pub fn update_host_interest(
    lan: &mut LanSession,
    traffic: &mut TrafficSim,
    people: &mut PeopleSim,
) -> InterestUpdate {
    if lan.role != Role::Host {
        return InterestUpdate::default();
    }
    let mut summary = InterestUpdate::default();
    traffic.lan_centers.clear();
    traffic.lan_eyes.clear();
    traffic.others.clear();
    traffic.other_blinkers.clear();
    people.lan_centers.clear();

    for peer in lan.peers() {
        if !peer.has_pose || !peer.has_info {
            continue;
        }
        let Some((position, body)) = vehicle_box(&peer.pose) else {
            continue;
        };
        summary.visible_players += 1;
        traffic.lan_centers.push(position);
        traffic
            .lan_eyes
            .push(Viewer::lan_player(position, f64::from(peer.pose.heading)));
        traffic.others.push((peer.pose.id, body));
        traffic.other_blinkers.insert(peer.pose.id, peer.pose.blinker);
        people.lan_centers.push(position);
    }

    for (player, requested) in lan.take_claims() {
        summary.boarding_claims += requested.len();
        let granted = people.hand_over(player, &requested);
        let granted_set: HashSet<u32> = granted.iter().copied().collect();
        let denied: Vec<u32> = requested
            .iter()
            .copied()
            .filter(|id| !granted_set.contains(id))
            .collect();
        summary.granted += granted.len();
        summary.denied += denied.len();
        lan.answer_claim(player, &granted, &denied);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_or_nonfinite_positions_cannot_poison_ai_physics() {
        let mut pose = Pose {
            bus: "Vehicles/Bus.bus".into(),
            x: 12.0,
            y: -30.0,
            z: 3.0,
            heading: 90.0,
            ..Default::default()
        };
        let (_, body) = vehicle_box(&pose).expect("legacy vehicle uses safe box");
        assert!((body.2 - 5.5).abs() < 0.001);
        pose.x = f64::NAN;
        assert!(vehicle_box(&pose).is_none());
        pose.x = 1.0e12;
        assert!(vehicle_box(&pose).is_none());
        pose.x = 1.0;
        pose.width = f32::NAN;
        pose.length = f32::INFINITY;
        pose.speed_kmh = f32::NAN;
        let (_, body) = vehicle_box(&pose).expect("invalid extents fall back");
        assert!((body.2 - 5.5).abs() < 0.001);
        assert!((body.3 - 1.25).abs() < 0.001);
        assert_eq!(body.4, 0.0);
    }

    #[test]
    fn connected_client_positions_are_the_authoritative_ai_interest_points() {
        use omsi_net::WorldInfo;
        use omsi_sim::ai_traffic::setup::RandomTypes;
        use omsi_sim::traffic::Network;
        use std::net::SocketAddr;
        use std::path::Path;
        use std::time::Duration;

        let mut host = LanSession::host(0, "Dedicated", WorldInfo::default(), true).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
        let mut a = LanSession::join_addr(vec![addr], None, "a", WorldInfo::default()).unwrap();
        let mut b = LanSession::join_addr(vec![addr], None, "b", WorldInfo::default()).unwrap();
        let pa = Pose {
            bus: "Vehicles/BusA.bus".into(),
            x: 100.0,
            y: 210.0,
            width: 2.5,
            length: 12.0,
            ..Default::default()
        };
        let pb = Pose {
            bus: "Vehicles/BusB.bus".into(),
            x: -50.0,
            y: 20.0,
            width: 2.5,
            length: 8.0,
            ..Default::default()
        };
        for _ in 0..180 {
            host.tick(0.06, &Pose::default());
            a.tick(0.06, &pa);
            b.tick(0.06, &pb);
            if host.peers().filter(|peer| peer.has_pose && peer.has_info && peer.pose.has_vehicle()).count() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        let mut people = PeopleSim::new(Path::new("."), 0);
        let status = update_host_interest(&mut host, &mut traffic, &mut people);
        assert_eq!(status.visible_players, 2);
        assert_eq!(traffic.lan_eyes.len(), 2);
        assert_eq!(traffic.others.len(), 2);
        assert_eq!(people.lan_centers.len(), 2);
        assert!(traffic.lan_centers.iter().any(|p| (p.x - 100.0).abs() < 0.01));
        assert!(traffic.lan_centers.iter().any(|p| (p.x + 50.0).abs() < 0.01));
        host.kick(a.my_id, "disconnected", false);
        let status = update_host_interest(&mut host, &mut traffic, &mut people);
        assert_eq!(status.visible_players, 1);
        assert_eq!(traffic.others.len(), 1);
        assert_eq!(people.lan_centers.len(), 1);
    }

    #[test]
    fn absent_vehicle_has_no_ai_obstacle() {
        assert!(vehicle_box(&Pose::default()).is_none());
    }
}

//! Renderer-free LAN publication of authoritative traffic light clocks.
//!
//! The graphical client receives normal WORLD datagrams, as the legacy LAN
//! host publishes once a second. Population and AI car sync are separate.

use glam::DVec3;
use omsi_net::world::{LightState, WorldFrame, LIGHTS_EVERY};
use omsi_net::{LanSession, Role};
use omsi_sim::ai_traffic::TrafficSim;

/// Same near-client radius as the graphical LAN world.
const SIGNAL_RADIUS: f64 = 450.0;
/// Do not let dense maps generate unbounded datagrams per client per tick.
const MAX_LIGHTS_PER_CLIENT: usize = 192;

#[derive(Default)]
pub struct HeadlessWorldSync {
    elapsed: f32,
    pub sent_bytes: u64,
}

fn light_frame(states: impl IntoIterator<Item = (i64, f64, bool)>) -> WorldFrame {
    let mut lights: Vec<LightState> = states
        .into_iter()
        .filter_map(|(object, time, held)| {
            (u32::try_from(object).is_ok() && time.is_finite() && time >= 0.0)
                .then_some(LightState { object, time, held })
        })
        .collect();
    lights.sort_by_key(|light| light.object);
    lights.dedup_by_key(|light| light.object);
    lights.truncate(MAX_LIGHTS_PER_CLIENT);
    WorldFrame { lights, ..Default::default() }
}

impl HeadlessWorldSync {
    /// Publish real CPU signal clocks to all clients near their buses.
    pub fn tick(&mut self, lan: &mut LanSession, traffic: &TrafficSim, dt: f32) -> usize {
        if lan.role != Role::Host || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.elapsed += dt;
        if self.elapsed < LIGHTS_EVERY {
            return 0;
        }
        self.elapsed = (self.elapsed - LIGHTS_EVERY).min(LIGHTS_EVERY);
        let clients: Vec<(u32, DVec3)> = lan
            .peers()
            .filter(|peer| peer.has_info && peer.has_pose && peer.pose.has_vehicle())
            .filter_map(|peer| {
                let pos = DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z);
                (pos.is_finite() && pos.x.abs() <= 1.0e8 && pos.y.abs() <= 1.0e8)
                    .then_some((peer.pose.id, pos))
            })
            .collect();
        let mut bytes = 0usize;
        for (id, center) in clients {
            let frame = light_frame(traffic.light_states(center, SIGNAL_RADIUS));
            if !frame.lights.is_empty() {
                bytes += lan.send_world(id, &frame);
            }
        }
        self.sent_bytes += bytes as u64;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_net::world;

    #[test]
    fn invalid_and_duplicate_signal_states_do_not_reach_client() {
        let frame = light_frame([
            (5, 23.5, false),
            (-5, 10.0, true),
            (7, f64::NAN, false),
            (5, 23.5, true),
            (u32::MAX as i64 + 1, 1.0, false),
            (2, 0.0, true),
        ]);
        assert_eq!(frame.lights.len(), 2);
        assert_eq!(frame.lights[0].object, 2);
        assert_eq!(frame.lights[1].object, 5);
        let encoded = world::encode(&frame, omsi_net::PROTOCOL as u8);
        assert!(!encoded.is_empty());
        let received = world::decode(&encoded[0], omsi_net::PROTOCOL as u8)
            .expect("production client protocol must read these clocks");
        assert_eq!(received.lights.len(), 2);
        assert_eq!(received.lights[0].object, 2);
        assert_eq!(received.lights[1].object, 5);
    }

    #[test]
    fn dense_signal_maps_have_a_bounded_snapshot() {
        let frame = light_frame((0..1000).rev().map(|id| (id, id as f64, false)));
        assert_eq!(frame.lights.len(), MAX_LIGHTS_PER_CLIENT);
        assert_eq!(frame.lights[0].object, 0);
    }
}

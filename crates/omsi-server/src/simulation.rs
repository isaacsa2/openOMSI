//! CPU-only traffic and passenger simulation loop.
//!
//! This deliberately reuses the production `TrafficSim` and `PeopleSim`
//! implementations from omsi-sim. No renderer, camera, sound mixer, local
//! player bus, GPU buffer or presentation scene is needed to advance a tick.
//!
//! This is not yet a replacement for the game server: map collision surfaces,
//! signal programs, timetables and LAN snapshot transfer must be fully wired
//! before a new host process can admit clients.

use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::people::PeopleSim;
use omsi_sim::world_data::WorldCell;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// Authoritative CPU simulation shared by a future dedicated host process.
pub struct CpuSimulation {
    pub world: Arc<WorldCell>,
    pub traffic: TrafficSim,
    pub people: PeopleSim,
}

impl CpuSimulation {
    pub fn new(root: &Path, world: Arc<WorldCell>, traffic: TrafficSim, max_people: usize) -> Self {
        Self {
            world,
            traffic,
            people: PeopleSim::new(root, max_people),
        }
    }

    /// Advance one fixed simulation tick. Snapshot a consistent WorldData
    /// before invoking the same traffic and passenger code as the client.
    /// The caller is responsible for scheduling real time and publishing LAN
    /// snapshots; neither step is done by drawing the world.
    pub fn step(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 || dt > 0.1 {
            log::warn!("headless simulation: ignoring invalid tick dt={dt}");
            return;
        }
        let started = Instant::now();
        let snapshot = self.world.snapshot();
        self.traffic.advance(dt, None);
        self.people.tick_inner(dt, &*snapshot, None, Some(&self.traffic));
        self.people
            .tick_done(dt, &*snapshot, started.elapsed().as_secs_f64() * 1000.0);
    }

    /// Call after the host has serialized its authoritative network updates.
    /// Visual events and AI sound notifications are unnecessary on a dedicated
    /// server and otherwise accumulate without a render/audio consumer.
    pub fn discard_presentation_events_after_publish(&mut self) {
        self.people.bodies.ops.clear();
        self.traffic.retired.clear();
        self.traffic.honked.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::ai_traffic::setup::RandomTypes;
    use omsi_sim::traffic::Network;
    use omsi_sim::world_data::WorldData;

    fn empty_simulation() -> CpuSimulation {
        let world = Arc::new(WorldCell::new(WorldData::default()));
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let traffic = crate::traffic::assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        CpuSimulation::new(Path::new("."), world, traffic, 0)
    }

    #[test]
    fn traffic_and_people_advance_on_the_same_cpu_world_snapshot() {
        let mut host = empty_simulation();
        assert!(host.people.people.is_empty());
        for _ in 0..60 {
            host.step(1.0 / 60.0);
            host.discard_presentation_events_after_publish();
        }
        assert!(host.people.people.is_empty());
        assert!(host.traffic.cars.is_empty());
        assert!((host.people.time - 1.0).abs() < 0.001);
        assert!(host.people.bodies.ops.is_empty());
    }

    #[test]
    fn invalid_step_cannot_corrupt_the_simulation_clock() {
        let mut host = empty_simulation();
        host.step(f32::NAN);
        host.step(-1.0);
        host.step(2.0);
        assert_eq!(host.people.time, 0.0);
        host.step(1.0 / 60.0);
        assert!(host.people.time > 0.0);
    }
}

//! Bounded AI vehicle population for the renderer-free experimental host.
//! Real vehicles run their native omsi-sim physics and script programs; no
//! graphical assets or fake client-side AI packets are generated.

use glam::DVec3;
use omsi_sim::ai_traffic::TrafficSim;
use omsi_sim::traffic::LaneKind;
use omsi_sim::world_data::WorldCell;
use omsi_sim::{SimClock, VehicleHost, VehicleInstance, VehicleType};
use std::sync::Arc;

pub const MAX_EXPERIMENTAL_TRAFFIC: usize = 32;
const SPAWN_INTERVAL: f32 = 1.0;
const SPAWN_RADIUS: f64 = 390.0;
const MIN_PLAYER_DISTANCE: f64 = 100.0;
const MIN_CAR_GAP: f64 = 24.0;

pub struct CpuTrafficPopulation {
    elapsed: f32,
    state: u64,
}

impl Default for CpuTrafficPopulation {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            state: 0xD1B5_4A32_0713_1EEB,
        }
    }
}

impl CpuTrafficPopulation {
    fn random(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Take a bounded number of candidates per interval and avoid spawning
    /// in sight of a driver or on a collision-occupied road. Only use locally
    /// installed, map-approved AI types already loaded by traffic::random_types.
    pub fn tick(
        &mut self,
        traffic: &mut TrafficSim,
        data: &Arc<WorldCell>,
        viewers: &[DVec3],
        target: usize,
        dt: f32,
    ) -> usize {
        if target == 0 || viewers.is_empty() || traffic.types.is_empty() {
            return 0;
        }
        self.elapsed += dt;
        if self.elapsed < SPAWN_INTERVAL {
            return 0;
        }
        self.elapsed %= SPAWN_INTERVAL;
        let budget = target
            .min(MAX_EXPERIMENTAL_TRAFFIC)
            .saturating_sub(traffic.cars.len())
            .min(2);
        let mut placed = 0;
        for _ in 0..budget {
            let near = viewers[(self.random() as usize) % viewers.len()];
            let candidates: Vec<usize> = traffic
                .net
                .lanes_starting_near(near, SPAWN_RADIUS)
                .into_iter()
                .filter(|&i| {
                    let lane = &traffic.net.lanes[i];
                    lane.kind == LaneKind::Street
                        && lane.length() > 35.0
                        && !lane.no_cars
                        && lane.density > 0.0
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let lane_id = candidates[(self.random() as usize) % candidates.len()];
            let lane = &traffic.net.lanes[lane_id];
            let distance = (lane.length() - 12.0).max(1.0);
            let along = 6.0 + ((self.random() % 10_000) as f32 / 10_000.0) * distance;
            let (position, _) = lane.at(along);
            if viewers
                .iter()
                .any(|at| (position - *at).truncate().length() < MIN_PLAYER_DISTANCE)
                || traffic
                    .cars
                    .iter()
                    .any(|car| (position - car.vehicle.position).truncate().length() < MIN_CAR_GAP)
            {
                continue;
            }
            let valid_ground = {
                let d = data.peek();
                let (key, _) = omsi_map::world_to_tile_local(position.x, position.y);
                d.ground.terrains.contains_key(&key) || d.ground.surfaces.contains_key(&key)
            };
            if !valid_ground {
                continue;
            }
            let eligible: Vec<_> = traffic
                .types
                .iter()
                .filter(|entry| entry.2 == LaneKind::Street && entry.1.is_finite() && entry.1 > 0.0)
                .map(|entry| Arc::clone(&entry.0))
                .collect();
            if eligible.is_empty() {
                break;
            }
            let ty: Arc<VehicleType> = eligible[(self.random() as usize) % eligible.len()].clone();
            let seed = self.random();
            let scheme = traffic.ai_scheme(&ty, seed >> 8);
            let mut host = VehicleHost::new(SimClock::default());
            host.paint_scheme = Some(scheme);
            let mut vehicle = VehicleInstance::new(ty.clone(), host);
            attach_cpu_ground(&mut vehicle, data);
            vehicle.apply_paint_vars(scheme);
            traffic.place_car(
                vehicle,
                LaneKind::Street,
                lane_id,
                along,
                ty,
                seed,
                scheme,
                None,
                None,
                None,
            );
            placed += 1;
        }
        placed
    }
}

/// Attach the same road/terrain and exact wheel-contact samplers used by
/// the game, but via shared immutable CPU map snapshots rather than a Scene.
pub fn attach_cpu_ground(vehicle: &mut VehicleInstance, data: &Arc<WorldCell>) {
    let ground = Arc::clone(data);
    vehicle.ground = Some(Arc::new(move |x, y| {
        let d = ground.peek();
        let (key, (lx, ly)) = omsi_map::world_to_tile_local(x, y);
        d.ground
            .surfaces
            .get(&key)
            .and_then(|surface| surface.sample_road_smooth(lx as f32, ly as f32))
            .or_else(|| {
                d.ground
                    .terrains
                    .get(&key)
                    .map(|terrain| terrain.sample(lx as f32, ly as f32))
            })
            .map(f64::from)
    }));
    let ground = Arc::clone(data);
    vehicle.contact =
        Some(
            Arc::new(move |x, y, top| ground.peek().ground.drive_probe(x, y, top))
                as Arc<dyn omsi_sim::rigid::Ground>,
        );
}

/// Construct an AI host with the actual server calendar and clock, not the
/// graphical application's default 09:00 time.
pub fn ai_host_clock(date: Option<&str>, time: &str, day_time: f64) -> SimClock {
    let mut clock = SimClock::starting(date, None, time);
    clock.time = day_time.rem_euclid(86_400.0);
    clock
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::ai_traffic::setup::RandomTypes;
    use omsi_sim::traffic::Network;
    use omsi_sim::world_data::WorldData;

    #[test]
    fn cannot_create_vehicles_without_connected_players_or_ai_types() {
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            std::path::Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            10,
        );
        let data = Arc::new(WorldCell::new(WorldData::default()));
        let mut population = CpuTrafficPopulation::default();
        assert_eq!(population.tick(&mut traffic, &data, &[], 10, 1.0), 0);
        assert_eq!(
            population.tick(&mut traffic, &data, &[DVec3::ZERO], 10, 1.0),
            0
        );
        assert!(traffic.cars.is_empty());
    }
}

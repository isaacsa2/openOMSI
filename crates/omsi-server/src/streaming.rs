//! Interest-based CPU tile selection for the experimental headless host.
//!
//! Prefer the tiles under connected players, then nearby neighborhoods,
//! without dropping the initial map entrypoint area. This is a bounded
//! selection policy; unloading world entities and AI remains separate work.

use omsi_map::global::GlobalCfg;
use std::collections::BTreeSet;

pub const STREAM_TILE_LIMIT: usize = 192;
pub const STREAM_NEIGHBOR_RADIUS: i32 = 2;

pub fn interest_tiles(
    global: &GlobalCfg,
    initial: &[(i32, i32)],
    player_positions: &[(f64, f64)],
    limit: usize,
) -> Vec<(i32, i32)> {
    if limit == 0 {
        return Vec::new();
    }
    let known: BTreeSet<_> = global.tiles.iter().map(|tile| (tile.x, tile.y)).collect();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut add = |key: (i32, i32)| {
        if selected.len() < limit && known.contains(&key) && seen.insert(key) {
            selected.push(key);
        }
    };
    let centers: Vec<_> = player_positions
        .iter()
        .filter(|(x, y)| {
            x.is_finite() && y.is_finite() && x.abs() <= 1.0e8 && y.abs() <= 1.0e8
        })
        .map(|&(x, y)| omsi_map::world_to_tile_local(x, y).0)
        .filter(|key| known.contains(key))
        .collect();

    // Never let a distant player lose their own tile to the initial 64.
    for &center in &centers {
        add(center);
    }
    // The original spawn neighborhood stays present for late joiners.
    for &key in initial {
        add(key);
    }
    // Expand one ring at a time across all players: a crowded server must
    // not allocate the entire tile budget to its first connected player.
    for radius in 1..=STREAM_NEIGHBOR_RADIUS {
        for &(cx, cy) in &centers {
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    if let (Some(x), Some(y)) = (cx.checked_add(dx), cy.checked_add(dy)) {
                        add((x, y));
                    }
                }
            }
        }
    }
    selected
}

/// Completed CPU map snapshot for the free-roam host. Loading is never
/// performed on the network tick thread.
pub struct StreamUpdate {
    pub keys: Vec<(i32, i32)>,
    pub signals: crate::signals::SignalNetwork,
    pub surfaces: crate::surfaces::SurfaceCounts,
    pub collisions: crate::collision::CollisionCounts,
}

pub struct StreamWorker {
    requests: std::sync::mpsc::Sender<Vec<(i32, i32)>>,
    results: std::sync::mpsc::Receiver<Option<StreamUpdate>>,
    worker: std::thread::JoinHandle<()>,
}

impl StreamWorker {
    /// Own the mutable CPU map on a separate thread. Shared WorldCell data is
    /// published via immutable snapshots and read by the running simulation.
    /// The restricted relay has no server-owned AI, so old entities cannot
    /// retain lane or surface references during a region replacement.
    pub fn start(mut world: crate::world::HostWorld) -> std::io::Result<Self> {
        let (requests, pending) = std::sync::mpsc::channel::<Vec<(i32, i32)>>();
        let (complete, results) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("omsi-cpu-tile-stream".into())
            .spawn(move || {
                while let Ok(keys) = pending.recv() {
                    let signals = world.network_with_traffic_lights(&keys);
                    if signals.network.lanes.is_empty() {
                        log::warn!(
                            "native tile stream: no road lanes in the selected region"
                        );
                        if complete.send(None).is_err() {
                            break;
                        }
                        continue;
                    }
                    let surfaces = world.install_spline_ground_surfaces(&keys);
                    world.install_stop_lists(&keys);
                    let collisions =
                        world.install_static_box_collisions(&keys, &signals.network.lanes);
                    world.prune_streamed_ground(&keys);
                    if complete
                        .send(Some(StreamUpdate {
                            keys,
                            signals,
                            surfaces,
                            collisions,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            results,
            worker,
        })
    }

    pub fn request(&self, keys: Vec<(i32, i32)>) -> bool {
        self.requests.send(keys).is_ok()
    }

    /// Some(None) indicates a rejected region. None means still processing.
    pub fn poll(&self) -> Option<Option<StreamUpdate>> {
        self.results.try_recv().ok()
    }

    pub fn stop(self) {
        let Self {
            requests,
            results: _,
            worker,
        } = self;
        drop(requests);
        if worker.join().is_err() {
            log::warn!("native tile stream worker panicked while shutting down");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_map::global::MapTileRef;

    fn fixture() -> GlobalCfg {
        let mut global = GlobalCfg::default();
        for x in 0..20 {
            for y in 0..20 {
                global.tiles.push(MapTileRef {
                    x,
                    y,
                    ..Default::default()
                });
            }
        }
        global
    }

    #[test]
    fn remote_driver_gets_their_tile_and_neighbors_not_just_spawn_area() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(15.5 * edge, 15.5 * edge)],
            40,
        );
        assert_eq!(&selected[..3], &[(15, 15), (0, 0), (1, 0)]);
        assert!(selected.contains(&(14, 15)));
        assert!(selected.contains(&(16, 16)));
        assert_eq!(selected.len(), selected.iter().copied().collect::<BTreeSet<_>>().len());
    }

    #[test]
    fn preserve_all_player_centers_before_the_small_tile_budget() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(2.5 * edge, 2.5 * edge), (17.5 * edge, 17.5 * edge)],
            2,
        );
        assert_eq!(selected, vec![(2, 2), (17, 17)]);
    }

    #[test]
    fn invalid_and_outside_coordinates_never_expand_the_world() {
        let global = fixture();
        let selected = interest_tiles(
            &global,
            &[(0, 0), (1, 0)],
            &[(f64::NAN, 0.0), (f64::INFINITY, 0.0), (1.0e9, 0.0)],
            STREAM_TILE_LIMIT,
        );
        assert_eq!(selected, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn valid_clients_fairly_share_one_ring_when_the_budget_is_tight() {
        let global = fixture();
        let edge = omsi_map::tile_size();
        let selected = interest_tiles(
            &global,
            &[],
            &[(5.5 * edge, 5.5 * edge), (14.5 * edge, 14.5 * edge)],
            6,
        );
        assert_eq!(&selected[..2], &[(5, 5), (14, 14)]);
        assert!(selected.iter().any(|&(x, y)| x.abs_diff(5) == 1 || y.abs_diff(5) == 1));
    }
}

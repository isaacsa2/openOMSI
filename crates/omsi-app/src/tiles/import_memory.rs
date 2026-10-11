//! Distance-based streaming for generated maps, with a conservative geometry estimate.
use super::{DVec3, Path, PathBuf, Streamer};

pub(super) const UNLOAD_MARGIN: f64 = 75.0;
const NEAR: f64 = 150.0;

pub(super) struct ImportMemory {
    sizes: Vec<((i32, i32), u64)>,
    requested: f64,
    budget: u64,
    last_radius: f64,
    centers: Vec<DVec3>,
}

impl ImportMemory {
    pub fn read(map: &Path, tiles: &[(i32, i32, PathBuf)], requested: f64) -> Option<Self> {
        // Only the importer's generated layout: ordinary OMSI maps keep their streaming.
        let models = map.join("proton-import/model");
        if tiles.is_empty() || !omsi_cfg::vfs::is_dir(&models) {
            return None;
        }
        let sizes = tiles
            .iter()
            .map(|(x, y, _)| {
                let bytes = omsi_cfg::vfs::file_size(&models.join(format!("tile_{x}_{y}.o3d")))?;
                // O3D v3 stores the same 32-byte vertices and slightly larger triangles than
                // the GPU. Include terrain and allocation slack; no meshes need decoding.
                Some((
                    (*x, *y),
                    bytes.saturating_add(bytes / 8).saturating_add(64 * 1024),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let vram = omsi_render::ADAPTER_VRAM_MB.load(std::sync::atomic::Ordering::Relaxed);
        let budget = geometry_budget(vram);
        log::info!(
            "generated map geometry allowance: {:.0} MB; requested radius {:.0} m",
            budget as f64 / 1e6,
            requested
        );
        Some(Self {
            sizes,
            requested,
            budget,
            last_radius: requested,
            centers: Vec::new(),
        })
    }

    pub fn radius(&mut self, centers: &[DVec3]) -> f64 {
        if centers.is_empty() {
            return self.last_radius;
        }
        // Density changes as the player moves, not every rendered frame.
        if centers.len() == self.centers.len()
            && centers
                .iter()
                .zip(&self.centers)
                .all(|(a, b)| a.distance_squared(*b) < 625.0)
        {
            return self.last_radius;
        }
        self.centers.clear();
        self.centers.extend_from_slice(centers);
        let costs = self
            .sizes
            .iter()
            .map(|(key, size)| (Streamer::nearest(centers, key.0, key.1), *size))
            .collect();
        let radius = radius_for_costs(costs, self.budget, self.requested);
        if (radius - self.last_radius).abs() >= 25.0 {
            log::info!("generated map streaming radius: {:.0} m (requested {:.0} m, geometry allowance {:.0} MB)", radius, self.requested, self.budget as f64 / 1e6);
        }
        self.last_radius = radius;
        radius
    }
}

fn geometry_budget(vram_mb: u64) -> u64 {
    // Leave most VRAM for textures, vehicles, frame targets and the driver. When the
    // adapter cannot report dedicated memory, use a modest geometry allowance.
    if vram_mb == 0 {
        384_000_000
    } else {
        vram_mb
            .saturating_mul(300_000)
            .clamp(128_000_000, 1_500_000_000)
    }
}

fn radius_for_costs(mut costs: Vec<(f64, u64)>, budget: u64, requested: f64) -> f64 {
    let near = NEAR.min(requested);
    costs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bytes = 0u64;
    for (distance, size) in costs {
        if distance > requested + UNLOAD_MARGIN {
            break;
        }
        bytes = bytes.saturating_add(size);
        if bytes > budget {
            // Exclude the whole distance ring, including the retained unload margin.
            // Ground near every camera/bus wins over the estimate: this is an allowance,
            // not a hard VRAM cap when a single mandatory tile is itself too large.
            return (distance - UNLOAD_MARGIN - 0.01).max(near).min(requested);
        }
    }
    requested
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heavy_map_keeps_nearby_ground_but_reduces_distant_residency() {
        let costs = vec![(0.0, 80), (150.0, 80), (350.0, 80), (600.0, 80)];
        let radius = radius_for_costs(costs.clone(), 200, 800.0);
        assert!((radius - 274.99).abs() < 0.001);
        assert_eq!(
            costs
                .iter()
                .filter(|(d, _)| *d <= radius + UNLOAD_MARGIN)
                .map(|(_, b)| b)
                .sum::<u64>(),
            160
        );
        assert_eq!(radius_for_costs(costs, 1, 800.0), NEAR);
    }

    #[test]
    fn small_maps_and_short_user_distances_keep_their_radius() {
        assert_eq!(
            radius_for_costs(vec![(0.0, 10), (300.0, 10)], 100, 800.0),
            800.0
        );
        assert_eq!(radius_for_costs(vec![(0.0, 200)], 100, 50.0), 50.0);
        assert_eq!(radius_for_costs(vec![(900.0, u64::MAX)], 100, 800.0), 800.0);
    }

    #[test]
    fn tied_distance_rings_are_excluded_together() {
        let radius = radius_for_costs(vec![(0.0, 10), (400.0, 30), (400.0, 70)], 100, 800.0);
        assert!(radius + UNLOAD_MARGIN < 400.0);
        assert_eq!(
            radius_for_costs(vec![(0.0, u64::MAX), (1.0, 10)], 100, 800.0),
            NEAR
        );
    }

    #[test]
    fn geometry_allowance_leaves_room_on_small_cards() {
        assert_eq!(geometry_budget(0), 384_000_000);
        assert_eq!(geometry_budget(4096), 1_228_800_000);
        assert_eq!(geometry_budget(8192), 1_500_000_000);
        assert_eq!(geometry_budget(256), 128_000_000);
        assert_eq!(geometry_budget(u64::MAX), 1_500_000_000);
    }
}

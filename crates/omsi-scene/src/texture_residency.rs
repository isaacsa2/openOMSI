//! Diagnostic view of texture residency, with categories that do not double-count
//! shared scenery/vehicle textures or the GPU's non-file-backed render targets.
use super::*;

/// An accounting snapshot, without retaining GPU handles or decoded pixels.
#[derive(Debug, Clone, Copy, Default)]
struct Residency {
    vehicle_count: usize,
    vehicle_bytes: u64,
    scenery_count: usize,
    scenery_bytes: u64,
    scenery_reduced: usize,
    other_bytes: u64,
    pending: usize,
    queued: usize,
    cpu_staged_bytes: usize,
}

impl Residency {
    /// Textures created for people, text/script displays, render targets, the sky and
    /// other resources that do not live in the file-backed vehicle/scenery maps.
    fn other(total: u64, vehicle: u64, scenery: u64) -> u64 {
        total.saturating_sub(vehicle.saturating_add(scenery))
    }

    fn describe(self) -> String {
        format!(
            "residency: {} vehicle textures {:.1} MB, {} scenery textures {:.1} MB ({} reduced), other textures {:.1} MB; CPU texture staging {:.1} MB; {} queued / {} in-flight texture upgrades",
            self.vehicle_count, self.vehicle_bytes as f64 / 1e6,
            self.scenery_count, self.scenery_bytes as f64 / 1e6, self.scenery_reduced,
            self.other_bytes as f64 / 1e6, self.cpu_staged_bytes as f64 / 1e6,
            self.queued, self.pending
        )
    }
}

impl World {
    /// Compact residency breakdown for OMSI_PROFILE / streaming logs. The total GPU
    /// texture allocation is read from Scene, not inferred from a VRAM setting.
    /// Locks are taken separately to avoid blocking tile/vehicle uploads.
    pub fn texture_residency_summary(&self, scene: &Scene) -> String {
        let (vehicle_count, vehicle_bytes) = {
            let textures = self.vehicle_textures.lock();
            (textures.len(), textures.values().map(|(id, _)| scene.texture_bytes_of(*id)).sum())
        };
        let (scenery_count, scenery_bytes, scenery_reduced, queued) = {
            let gpu = self.gpu.lock();
            (gpu.textures.len(), gpu.textures.values().map(|e| e.bytes).sum(),
                gpu.textures.values().filter(|e| e.dropped > 0).count(),
                gpu.wants_restore.len() + gpu.wants_upgrade.len())
        };
        let pending = self.upgrades_pending.lock().len();
        let cpu_staged_bytes = self.textures.held_bytes();
        let total = scene.gpu_bytes().0;
        Residency {
            vehicle_count,
            vehicle_bytes,
            scenery_count,
            scenery_bytes,
            scenery_reduced,
            other_bytes: Residency::other(total, vehicle_bytes, scenery_bytes),
            pending, queued, cpu_staged_bytes,
        }.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::Residency;

    #[test]
    fn overlapping_or_delayed_usage_accounting_never_underflows() {
        assert_eq!(Residency::other(900, 200, 400), 300);
        assert_eq!(Residency::other(500, 400, 200), 0);
        assert_eq!(Residency::other(0, 0, 0), 0);
        assert_eq!(Residency::other(usize::MAX as u64, u64::MAX, u64::MAX), 0);
    }

    #[test]
    fn summary_identifies_reduced_textures_and_pending_work() {
        let summary = Residency { scenery_reduced: 3, pending: 2, queued: 4, ..Default::default() }.describe();
        assert!(summary.contains("3 reduced"));
        assert!(summary.contains("4 queued / 2 in-flight"));
    }
}

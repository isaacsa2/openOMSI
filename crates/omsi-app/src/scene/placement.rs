//! Private staging for cancellable interactive vehicle placement.
use super::*;

impl World {
    /// An interactive request owns its staged GPU data until it is accepted. A
    /// cancelled placement must not leave unused meshes/textures in the fleet cache.
    pub fn placement_prefetch(&self, renderer: &Renderer) -> VehiclePrefetch {
        VehiclePrefetch {
            ready: Arc::new(Mutex::new(PreparedVehicles::default())),
            ..self.vehicle_prefetch(renderer)
        }
    }

    /// The finished worker no longer writes its private staging area. Existing
    /// fleet results win on duplicate keys, and the unused copies drop here.
    pub fn accept_placement_prefetch(&self, staged: VehiclePrefetch) {
        let PreparedVehicles { meshes, textures } = std::mem::take(&mut *staged.ready.lock());
        let mut ready = self.vehicle_ready.lock();
        for (key, value) in meshes {
            ready.meshes.entry(key).or_insert(value);
        }
        for (key, value) in textures {
            ready.textures.entry(key).or_insert(value);
        }
    }

}

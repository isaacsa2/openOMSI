//! Picking shares immutable local-space triangle bounds between vehicle instances.

use super::*;

impl VehicleMesh {
    pub fn cone_triangles(
        &self,
        origin: Vec3,
        axis: Vec3,
        tan_half: f32,
        transform: &Mat4,
    ) -> Vec<u32> {
        if self.data.indices.len() <= 24 {
            return omsi_geometry::cone_triangles(origin, axis, tan_half, &self.data, transform);
        }
        self.ray_bounds
            .get_or_init(|| Arc::new(omsi_geometry::MeshRaycast::new(&self.data)))
            .cone_triangles(origin, axis, tan_half, &self.data, transform)
    }

    pub fn prepare_picking(&self) {
        if self.data.indices.len() > 24 {
            self.ray_bounds
                .get_or_init(|| Arc::new(omsi_geometry::MeshRaycast::new(&self.data)));
        }
    }

    pub fn ray_distance(&self, origin: Vec3, direction: Vec3, transform: &Mat4) -> Option<f32> {
        // Tiny switches need no tree; large body meshes otherwise scan every triangle
        // again for hover names, scenery occlusion and VR cursor placement.
        if self.data.indices.len() <= 24 {
            return omsi_geometry::ray_mesh(origin, direction, &self.data, transform);
        }
        self.ray_bounds
            .get_or_init(|| Arc::new(omsi_geometry::MeshRaycast::new(&self.data)))
            .hit(origin, direction, &self.data, transform)
    }
}

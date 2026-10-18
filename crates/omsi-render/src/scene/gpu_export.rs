//! Export the shipping scene's CPU draw metadata into the backend-neutral
//! command contract shared with BGFX. No wgpu device/queue/command buffer
//! handles escape this boundary.
//!
//! This exports normal visible mesh instances and their material-index runs
//! while keeping phase and creation order. Actual pass sorting, terrain
//! transmap depth, exposure, UI, particles, reflections, XR and other effects
//! still need to be ported before production use.

use super::{Camera, Scene};
use crate::AlphaMode;
use glam::{Mat4, Vec3};
use omsi_geometry::gpu_contract::{
    BlendMode, DepthMode, Draw, FrameCommands, MaterialUpload, ResourceId, View, ViewCamera,
};

/// Logical GPU mesh/material IDs use the source slot index plus one. BGFX's
/// uploader will need a matching id-to-resource mapping in a future step.
impl Scene {
    /// Stable CPU-side material descriptions for a future BGFX renderer.
    /// The same source material slot + 1 used by native_frame_snapshot is
    /// retained here, so material binds can be updated without rebuilding the
    /// scene. Texture slot + 1 is the logical source ID: the host must decode
    /// and upload its original image before registering the material.
    ///
    /// This is ONLY the diffuse/color subset. Night maps, environment,
    /// lighting, CTC repaint, transmap, etc. have not been ported.
    pub fn native_material_snapshot(&self) -> Result<Vec<(ResourceId, MaterialUpload)>, String> {
        let mut out = Vec::with_capacity(self.materials.len());
        for (index, material) in self.materials.iter().enumerate() {
            let id = ResourceId(index as u64 + 1);
            let upload = MaterialUpload {
                diffuse_texture: material.texture.map(|index| ResourceId(index as u64 + 1)),
                color: material.color,
            };
            upload.validate()
                .map_err(|e| format!("scene material {index}: {e}"))?;
            out.push((id, upload));
        }
        Ok(out)
    }

    pub fn native_frame_snapshot(
        &self,
        camera: &Camera,
        viewport: (u32, u32),
        homogeneous_depth: bool,
    ) -> Result<FrameCommands, String> {
        let (w, h) = viewport;
        if w == 0 || h == 0 || w > u16::MAX as u32 || h > u16::MAX as u32 {
            return Err("invalid viewport for native BGFX frame".into());
        }
        if !camera.fov_deg.is_finite() || camera.fov_deg <= 0.0
            || camera.fov_deg >= 179.0 || camera.near <= 0.0
            || camera.far <= camera.near
            || !camera.near.is_finite() || !camera.far.is_finite()
            || !camera.position.is_finite()
        {
            return Err("invalid frame camera".into());
        }
        let aspect = w as f32 / h as f32;
        let view = Mat4::look_to_rh(Vec3::ZERO, camera.forward(), camera.up());
        // Preserve reverse Z, with the API-specific NDC depth range.
        let projection = if homogeneous_depth {
            Mat4::perspective_rh_gl(camera.fov_deg.to_radians(), aspect, camera.far, camera.near)
        } else {
            Mat4::perspective_rh(camera.fov_deg.to_radians(), aspect, camera.far, camera.near)
        };
        let mut draws = Vec::new();
        let mut indices: Vec<usize> = (0..self.instances.len())
            .filter(|&i| self.instances[i].visible)
            .collect();
        indices.sort_unstable_by_key(|&i| {
            let instance = &self.instances[i];
            (instance.render_phase as u8, instance.draw_order)
        });
        for i in indices {
            let instance = &self.instances[i];
            let Some(mesh) = self.meshes.get(instance.mesh) else { continue };
            if mesh.ranges.is_empty() { continue; }
            let local = (instance.origin - camera.position).as_vec3();
            let model = Mat4::from_translation(local) * instance.transform;
            if !model.is_finite() { continue; }
            for (first, count, slot) in mesh.ranges.iter().copied() {
                if count == 0 { continue; }
                let index = slot as usize;
                let Some(&material_id) = instance.materials.get(index) else { continue };
                if instance.slot_alpha.get(index).is_some_and(|a| *a <= 0.0) { continue; }
                let Some(material) = self.materials.get(material_id) else { continue };
                let blend = match material.alpha {
                    AlphaMode::Opaque => BlendMode::Opaque,
                    AlphaMode::Test => BlendMode::AlphaTest,
                    AlphaMode::Blend => BlendMode::AlphaBlend,
                };
                let depth = if material.no_z_check {
                    DepthMode::Disabled
                } else if material.no_z_write {
                    DepthMode::TestOnly
                } else {
                    DepthMode::TestWrite
                };
                draws.push(Draw {
                    mesh: ResourceId(instance.mesh as u64 + 1),
                    material: ResourceId(material_id as u64 + 1),
                    view: View::Main,
                    blend,
                    depth,
                    range: Some((first, count)),
                    transform: model.to_cols_array(),
                    authored_order: instance.draw_order,
                });
            }
        }
        let frame = FrameCommands {
            viewport,
            cameras: vec![ViewCamera {
                view: View::Main,
                world_to_camera: view.to_cols_array(),
                camera_to_clip: projection.to_cols_array(),
                reversed_z: true,
            }],
            draws,
        };
        frame.validate().map_err(str::to_owned)?;
        Ok(frame)
    }
}

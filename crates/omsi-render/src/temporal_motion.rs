//! Frame history for DLSS motion vectors in the modular renderer.
//!
//! The public Criisko/openOMSI PR #1 indexed last frame's model buffer by GPU
//! entry number. Here an entry can be rebuilt, removed or reused: match a
//! stable instance draw_order together with its material slot instead.
//! This module does not switch the renderer's antialiasing or output path.
use crate::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// The original scene entry point vs_depth cannot be called from another
/// entry point in WGSL. Convert just the *separate diagnostic shader copy* into
/// an ordinary helper, retaining the exact same depth, alpha and surface-bias
/// calculation. The normal scene shader remains byte-for-byte unchanged.
pub(crate) fn shader_source() -> String {
    const ENTRY: &str = "fn vs_depth(in: VsIn) -> DepthOut {";
    let mut text = scene_shader_source(false);
    let start = text.find(ENTRY).expect("depth shader entry point changed");
    let attr = text[..start].rfind("@vertex").expect("depth vertex attribute missing");
    assert!(text[attr + "@vertex".len()..start].trim().is_empty(),
        "depth vertex function attributes changed");
    text.replace_range(attr..start + ENTRY.len(), "fn dlss_depth_helper(in: VsIn) -> DepthOut {");
    format!("{}\n{}", text, include_str!("temporal_motion.wgsl"))
}

static NEXT_SCENE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn new_scene_id() -> u64 {
    NEXT_SCENE_ID.fetch_add(1, Ordering::Relaxed)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MotionUniform {
    pub current_vp: [[f32; 4]; 4],
    pub previous_vp: [[f32; 4]; 4],
    pub previous_vp_here: [[f32; 4]; 4],
    pub current_inv_vp: [[f32; 4]; 4],
    /// x/y = rendering resolution; z = history reset; w = model entry count.
    pub params: [f32; 4],
}

#[derive(Clone)]
struct PreviousFrame {
    scene_id: u64,
    camera: Camera,
    origin: DVec3,
    resolution: (u32, u32),
    models: HashMap<(u64, u32), Mat4>,
}

#[derive(Default)]
pub(crate) struct MotionHistory {
    previous: Option<PreviousFrame>,
    frame_index: u64,
}

pub(crate) struct PreparedMotion {
    pub uniform: MotionUniform,
    pub previous_models: Vec<[[f32; 4]; 4]>,
    pub reset: bool,
    pub matched_models: usize,
    pub jitter_px: [f32; 2],
}

pub(crate) fn halton(mut index: u32, base: u32) -> f32 {
    let (mut denom, mut result) = (1.0f32, 0.0f32);
    while index != 0 {
        denom /= base as f32;
        result += denom * (index % base) as f32;
        index /= base;
    }
    result
}

pub(crate) fn jitter_pixels(frame: u64, phase_count: u32) -> [f32; 2] {
    let i = (frame % phase_count.max(1) as u64) as u32 + 1;
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

/// A new entry did not exist in the previous frame: assume it did not move.
/// Convert its current matrix to the previous floating-origin coordinates so
/// a camera pan still produces correct motion.
fn previous_or_current(
    previous: Option<Mat4>,
    current: Mat4,
    current_origin: DVec3,
    previous_origin: DVec3,
) -> Mat4 {
    previous.unwrap_or_else(|| {
        Mat4::from_translation((current_origin - previous_origin).as_vec3()) * current
    })
}

impl MotionHistory {
    pub(crate) fn next_jitter(&self) -> [f32; 2] {
        jitter_pixels(self.frame_index, 8)
    }
    pub(crate) fn prepare(
        &mut self,
        scene: &Scene,
        camera: &Camera,
        resolution: (u32, u32),
    ) -> PreparedMotion {
        let resolution = (resolution.0.max(1), resolution.1.max(1));
        let aspect = resolution.0 as f32 / resolution.1 as f32;
        let current_vp = camera.view_proj(aspect, scene.render_origin);
        let previous = self.previous.as_ref();
        // History is never reused across a different scene, camera teleport,
        // sharp turn, FOV/clip change, or resolution change.
        let reset = previous.is_none_or(|p| {
            p.scene_id != scene.scene_id
                || p.resolution != resolution
                || (p.camera.position - camera.position).length() > 30.0
                || p.camera.forward().dot(camera.forward()) < 0.8
                || (p.camera.fov_deg - camera.fov_deg).abs() > 0.1
                || (p.camera.near - camera.near).abs() > 0.001
                || (p.camera.far - camera.far).abs() > 1.0
        });
        let before = previous.filter(|_| !reset);
        let previous_origin = before.map_or(scene.render_origin, |p| p.origin);
        let previous_vp = before.map_or(current_vp, |p| {
            p.camera.view_proj(aspect, p.origin)
        });
        let previous_vp_here = before.map_or(current_vp, |p| {
            p.camera.view_proj(aspect, scene.render_origin)
        });
        // Model entries are per (instance, material slot). Their GPU numbers can
        // change on a scene rebuild, so match by stable draw_order and slot.
        let mut previous_models = scene.cpu_models.clone();
        let mut matched_models = 0;
        for inst in &scene.instances {
            for slot in 0..inst.slot_alpha.len() {
                let entry = inst.base as usize + slot;
                let Some(matrix) = scene.cpu_models.get(entry) else { continue };
                let current = Mat4::from_cols_array_2d(matrix);
                let earlier = before.and_then(|p| {
                    p.models.get(&(inst.draw_order, slot as u32)).copied()
                });
                if earlier.is_some() { matched_models += 1; }
                previous_models[entry] = previous_or_current(
                    earlier, current, scene.render_origin, previous_origin
                ).to_cols_array_2d();
            }
        }
        let jitter_px = jitter_pixels(self.frame_index, 8);
        self.frame_index += 1;
        PreparedMotion {
            uniform: MotionUniform {
                current_vp: current_vp.to_cols_array_2d(),
                previous_vp: previous_vp.to_cols_array_2d(),
                previous_vp_here: previous_vp_here.to_cols_array_2d(),
                current_inv_vp: current_vp.inverse().to_cols_array_2d(),
                params: [resolution.0 as f32, resolution.1 as f32, if reset { 1.0 } else { 0.0 }, previous_models.len() as f32],
            },
            previous_models,
            reset,
            matched_models,
            jitter_px,
        }
    }

    /// Call only after the window view is submitted; never let a mirror or a
    /// headset eye overwrite the previous desktop camera or model transforms.
    pub(crate) fn finish(&mut self, scene: &Scene, camera: &Camera, resolution: (u32, u32)) {
        let mut models = HashMap::with_capacity(scene.uploaded_entries as usize);
        for inst in &scene.instances {
            for slot in 0..inst.slot_alpha.len() {
                let entry = inst.base as usize + slot;
                if let Some(m) = scene.cpu_models.get(entry) {
                    models.insert((inst.draw_order, slot as u32), Mat4::from_cols_array_2d(m));
                }
            }
        }
        self.previous = Some(PreviousFrame {
            scene_id: scene.scene_id,
            camera: *camera,
            origin: scene.render_origin,
            resolution,
            models,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn halton_jitter_stays_within_half_pixel() {
        assert!((halton(1, 2) - 0.5).abs() < f32::EPSILON);
        assert!((halton(2, 2) - 0.25).abs() < f32::EPSILON);
        assert!((halton(3, 3) - 1.0 / 9.0).abs() < 1e-6);
        for i in 0..128 {
            let [x,y] = jitter_pixels(i, 8);
            assert!((-0.5..=0.5).contains(&x) && (-0.5..=0.5).contains(&y));
        }
    }
    #[test]
    fn previous_matrix_respects_floating_origin() {
        let m = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
        let old = previous_or_current(None, m, DVec3::new(200.0, 100.0, 0.0), DVec3::new(100.0, 100.0, 0.0));
        assert!((old.w_axis.x - 103.0).abs() < 0.001);
        assert!((old.w_axis.y - 4.0).abs() < 0.001);
    }
    #[test]
    fn previous_transform_keeps_actual_object_motion() {
        let old = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let now = Mat4::from_translation(Vec3::new(4.0, 0.0, 0.0));
        let found = previous_or_current(Some(old), now, DVec3::ZERO, DVec3::ZERO);
        assert_eq!(found, old);
    }
    #[test]
    fn motion_shader_parses_and_validates_with_current_scene() {
        let text = shader_source();
        let module = naga::front::wgsl::parse_str(&text)
            .expect("current scene plus DLSS motion shader should parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("DLSS motion shader should validate");
    }
    #[test]
    fn dlss_presentation_shader_is_valid_wgsl() {
        let shader = include_str!("dlss_present.wgsl");
        let module = naga::front::wgsl::parse_str(shader)
            .expect("DLSS result presentation WGSL should parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("DLSS result presentation WGSL should validate");
    }

    #[test]
    fn motion_uniform_matches_wgsl_structure() {
        assert_eq!(std::mem::size_of::<MotionUniform>(), 272);
        assert_eq!(std::mem::align_of::<MotionUniform>(), 4);
    }
}

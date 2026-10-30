//! Motion vectors (OMSI_MOTION_VECTORS, experimental): for every pixel of the window's 3D
//! picture, where it was on the screen a frame ago (motion.wgsl). Used by the temporal
//! FSR 2.2.1 WGSL upscaler and standalone TAA; OMSI_DEBUG_MOTION visualizes the vectors.
//! When no temporal upscaler or debug flag requests them, this pass is not created.
//!
//! Worked out from the depth of the main pass where it is single-sampled and readable (the
//! prepass's, shared with the main pass: `shares_prepass_depth`), read as a plain float texture
//! so that OpenGL takes it too. The world is taken as still and the camera as moving, except
//! the player's own vehicle, carried back with it (so the cab does not smear).

use super::*;
use crate::pipelines::common::*;

/// The vectors' format: last frame's UV minus this frame's, in two halves.
pub(crate) const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MotionParams {
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    size: [f32; 4],
    to_body: [[f32; 4]; 4],
    from_body_prev: [[f32; 4]; 4],
    vehicle_box: [f32; 4],
    vehicle_centre: [f32; 4],
    flags: [f32; 4],
}

/// The motion vectors of one picture size.
pub(crate) struct MotionTarget {
    pub(crate) size: (u32, u32),
    // (kept with its view; read back in the tests)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    debug_bg: wgpu::BindGroup,
}

/// A single desktop view; mirrors and secondary viewports never advance its history.
#[derive(Clone, Copy)]
struct PreviousView {
    vp: Mat4,
    origin: DVec3,
    body: Option<(DVec3, Mat4)>,
    camera: Camera,
    size: (u32, u32),
    at: std::time::Instant,
}

/// Reject stale history on teleport, a camera cut, resolution change, projection edit or stall.
fn reusable_previous(p: &PreviousView, c: &Camera, size: (u32, u32), now: std::time::Instant) -> bool {
    p.size == size
        && p.vp.is_finite()
        && p.origin.is_finite()
        && now.saturating_duration_since(p.at).as_secs_f32() < 0.5
        && (p.camera.position - c.position).length() < 50.0
        && p.camera.forward().dot(c.forward()) > 0.5
        && (p.camera.fov_deg - c.fov_deg).abs() < 0.01
        && p.camera.near == c.near
        && p.camera.far == c.far
}

pub(crate) struct MotionVectors {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    debug_pipeline: Option<wgpu::RenderPipeline>,
    debug_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    pub(crate) target: Option<MotionTarget>,
    /// Last frame's view-projection, render origin and player vehicle's body (position,
    /// rotation).
    prev: Option<PreviousView>,
    /// Why the last frame had none (logged when it changes): "" while they are drawn.
    skipped: &'static str,
    /// This frame produced valid GPU inputs for a temporal consumer.
    pub(crate) frame_ready: bool,
    /// The first frame after history invalidation must reset a temporal upscaler.
    pub(crate) frame_reset: bool,
}

impl MotionVectors {
    /// The passes for a window of `format` (the debug view draws onto its 3D picture).
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> MotionVectors {
        log::info!("renderer: compiling the motion vectors shader");
        let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("motion vectors"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../motion.wgsl").into()),
        });
        let texture_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion vectors"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), texture_entry(1, wgpu::TextureSampleType::Float { filterable: false })],
        });
        let debug_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion vectors debug"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT), texture_entry(2, wgpu::TextureSampleType::Float { filterable: false })],
        });
        let pl = |label, layout: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 })
        };
        let pipeline = no_vertex_pipeline("motion vectors", &pl("motion vectors", &layout), &shader, "fs_motion", &[target(MOTION_FORMAT, None)]).create(device);
        let debug_pipeline = omsi_cfg::flags::OMSI_DEBUG_MOTION.is_set().then(|| {
            no_vertex_pipeline("motion vectors debug", &pl("motion vectors debug", &debug_layout), &shader, "fs_debug", &[target(format, Some(wgpu::BlendState::ALPHA_BLENDING))]).create(device)
        });
        let params = uniform_buffer(device, "motion vectors", std::mem::size_of::<MotionParams>() as u64);
        MotionVectors { pipeline, layout, debug_pipeline, debug_layout, params, target: None, prev: None, skipped: "", frame_ready: false, frame_reset: true }
    }

    /// The vectors' texture for pictures of `size` (made again when the size changes).
    fn target(&mut self, device: &wgpu::Device, size: (u32, u32)) -> &MotionTarget {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("motion vectors"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: MOTION_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let debug_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("motion vectors debug"),
                layout: &self.debug_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view) },
                ],
            });
            self.target = Some(MotionTarget { size, texture, view, debug_bg });
        }
        self.target.as_ref().expect("motion vectors target")
    }

    /// No last frame to move from (a cut: a new view, a teleport): the next picture's
    /// vectors are all zero, and it becomes the last frame.
    pub(crate) fn reset(&mut self) {
        self.prev = None;
        self.frame_ready = false;
        self.frame_reset = true;
    }
}

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn history_is_rejected_on_resizes_teleports_camera_cuts_and_stalls() {
        let c = Camera { position: DVec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 2000.0 };
        let now = std::time::Instant::now();
        let p = PreviousView { vp: c.view_proj(1.5, DVec3::ZERO), origin: DVec3::ZERO, body: None, camera: c, size: (192, 128), at: now };
        assert!(reusable_previous(&p, &c, (192, 128), now));
        assert!(!reusable_previous(&p, &c, (256, 128), now));
        let mut moved = c; moved.position.x += 55.0;
        assert!(!reusable_previous(&p, &moved, (192, 128), now));
        let mut rotated = c; rotated.yaw += 100.0;
        assert!(!reusable_previous(&p, &rotated, (192, 128), now));
        let mut zoomed = c; zoomed.fov_deg += 5.0;
        assert!(!reusable_previous(&p, &zoomed, (192, 128), now));
        assert!(!reusable_previous(&p, &c, (192, 128), now + std::time::Duration::from_secs(1)));
    }
}

impl Renderer {
    /// The motion vectors of the window's picture (after the main pass): when they are on,
    /// for the lead view, and when the main pass's depth is the readable single-sampled one.
    /// Whether this frame's motion vectors can be worked out: the prepass's depth, of the
    /// picture's size, shared with the main pass (`encode_motion` and FSR 2 ask the same).
    pub(crate) fn motion_depth_ready(&self, f: &FrameCtx, plan: &DrawPlan) -> bool {
        self.targets.ao.as_ref().is_some_and(|a| a.size == (f.width, f.height)) && self.shares_prepass_depth(f, plan)
    }

    pub(crate) fn encode_motion(&mut self, encoder: &mut wgpu::CommandEncoder, f: &FrameCtx, plan: &DrawPlan, view_proj: Mat4, timers: &mut PassTimers) {
        // A mirror, XR eye or offscreen thumbnail may share the renderer but
        // must never advance the desktop's motion history.
        if self.motion.is_none() || !f.with_overlays || f.projection.is_some() || f.second_eye {
            return;
        }
        let depth = self.targets.ao.as_ref().map(|a| a.depth_view.clone());
        let Some(depth) = depth.filter(|_| self.motion_depth_ready(f, plan)) else {
            // (no readable depth this frame: the next one starts afresh; and the log says why)
            let why = if self.options.msaa > 1 {
                "MSAA is on (turn it off)"
            } else if f.puddles_wanted {
                "puddles are in the picture"
            } else if plan.has_presurface {
                "a presurface (an excavation) is in the picture"
            } else {
                "there is no depth prepass"
            };
            let m = self.motion.as_mut().expect("motion vectors");
            if m.skipped != why {
                log::info!("motion vectors: none while {why}");
                m.skipped = why;
            }
            m.reset();
            return;
        };
        if let Some(m) = self.motion.as_mut().filter(|m| !m.skipped.is_empty() || m.prev.is_none() && m.target.is_none()) {
            log::info!("motion vectors: drawn ({}x{})", f.width, f.height);
            m.skipped = "";
        }
        let (w, h) = (f.width, f.height);
        let ro = f.ro;
        let m = self.motion.as_mut().expect("motion vectors");
        // History is valid only for this camera, projection and viewport.
        let now = std::time::Instant::now();
        let previous = m.prev.filter(|p| reusable_previous(p, f.camera, (w, h), now));
        m.frame_reset = previous.is_none();
        m.frame_ready = true;
        let (prev_vp, known) = match previous {
            Some(p) => (p.vp * Mat4::from_translation((ro - p.origin).as_vec3()), true),
            None => (view_proj, false),
        };
        // (the body's transforms in this frame's render-origin coordinates)
        let body = |(o, rot): (DVec3, Mat4)| Mat4::from_translation((o - ro).as_vec3()) * rot;
        let (to_body, from_body_prev, vehicle_box, vehicle_centre) = match (f.lighting.inside, f.lighting.inside.as_ref().zip(f.lighting.inside_rotation).map(|(b, rot)| (b.0, rot)), previous.and_then(|p| p.body)) {
            (Some((_, _, bb)), Some(now), Some(before)) => (
                body(now).inverse(),
                body(before),
                [bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5, 1.0],
                [bb[3], bb[4], bb[5], 0.0],
            ),
            _ => (Mat4::IDENTITY, Mat4::IDENTITY, [0.0; 4], [0.0; 4]),
        };
        let debug_px = omsi_cfg::flags::OMSI_DEBUG_MOTION.parse::<f32>().filter(|v| *v > 0.0).unwrap_or(8.0);
        let params = MotionParams {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            size: [w as f32, h as f32, 1.0 / w.max(1) as f32, 1.0 / h.max(1) as f32],
            to_body: to_body.to_cols_array_2d(),
            from_body_prev: from_body_prev.to_cols_array_2d(),
            vehicle_box,
            vehicle_centre,
            flags: [known as u8 as f32, debug_px, 0.0, 0.0],
        };
        self.queue.write_buffer(&m.params, 0, bytemuck::bytes_of(&params));
        m.prev = Some(PreviousView {
            vp: view_proj,
            origin: ro,
            body: f.lighting.inside.as_ref().zip(f.lighting.inside_rotation).map(|(b, rot)| (b.0, rot)),
            camera: *f.camera,
            size: (w, h),
            at: now,
        });
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motion vectors"),
            layout: &m.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: m.params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&depth) },
            ],
        });
        let view = m.target(&self.device, (w, h)).view.clone();
        let m = self.motion.as_ref().expect("motion vectors");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("motion vectors"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "motion vectors"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&m.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }

    /// OMSI_DEBUG_MOTION: the vectors drawn over the 3D picture, at its own size (before it
    /// is scaled up to the window and the HUD put on it).
    pub(crate) fn encode_motion_debug(&self, encoder: &mut wgpu::CommandEncoder, f: &FrameCtx) {
        let Some(m) = self.motion.as_ref() else { return };
        let (Some(pipeline), Some(t)) = (m.debug_pipeline.as_ref(), m.target.as_ref()) else { return };
        if !f.lead_view || t.size != (f.width, f.height) {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("motion vectors debug"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: f.scene_view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &t.debug_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}

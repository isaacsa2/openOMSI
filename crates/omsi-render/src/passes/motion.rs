//! Motion vectors (OMSI_MOTION_VECTORS, experimental): for every pixel of the window's 3D
//! picture, where it was on the screen a frame ago (motion.wgsl) - the first step towards a
//! temporal upscaler (FSR 2) or anti-aliasing, which take it with the depth and the colour.
//! Nothing reads it yet but the debug view (OMSI_DEBUG_MOTION): the picture is unchanged.
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
    vehicle_now: [f32; 4],
    vehicle_prev: [f32; 4],
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

pub(crate) struct MotionVectors {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    debug_pipeline: Option<wgpu::RenderPipeline>,
    debug_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    pub(crate) target: Option<MotionTarget>,
    /// Last frame's view-projection, render origin and player vehicle (origin, heading deg).
    prev: Option<(Mat4, DVec3, Option<(DVec3, f64)>)>,
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
        MotionVectors { pipeline, layout, debug_pipeline, debug_layout, params, target: None, prev: None }
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
    }
}

impl Renderer {
    /// The motion vectors of the window's picture (after the main pass): when they are on,
    /// for the lead view, and when the main pass's depth is the readable single-sampled one.
    pub(crate) fn encode_motion(&mut self, encoder: &mut wgpu::CommandEncoder, f: &FrameCtx, plan: &DrawPlan, view_proj: Mat4, timers: &mut PassTimers) {
        if self.motion.is_none() || !f.lead_view {
            return;
        }
        let depth = self.targets.ao.as_ref().filter(|a| a.size == (f.width, f.height)).map(|a| a.depth_view.clone());
        let Some(depth) = depth.filter(|_| self.shares_prepass_depth(f, plan)) else {
            // (no readable depth this frame: the next one starts afresh)
            self.motion.as_mut().expect("motion vectors").reset();
            return;
        };
        let (w, h) = (f.width, f.height);
        let ro = f.ro;
        let m = self.motion.as_mut().expect("motion vectors");
        // (a long frame or none before: no motion taken from it)
        let (prev_vp, known) = match m.prev {
            Some((vp, prev_ro, _)) if f.dt < 0.5 => (vp * Mat4::from_translation((ro - prev_ro).as_vec3()), true),
            _ => (view_proj, false),
        };
        let vrel = |o: DVec3| (o - ro).as_vec3();
        let (vehicle_now, vehicle_prev, vehicle_box, vehicle_centre) = match (f.lighting.inside, m.prev.and_then(|p| p.2)) {
            (Some((o, hdg, bb)), Some((po, ph))) => (
                vrel(o).extend((hdg as f32).to_radians()).to_array(),
                vrel(po).extend((ph as f32).to_radians()).to_array(),
                [bb[0] * 0.5, bb[1] * 0.5, bb[2] * 0.5, 1.0],
                [bb[3], bb[4], bb[5], 0.0],
            ),
            _ => ([0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]),
        };
        let debug_px = omsi_cfg::flags::OMSI_DEBUG_MOTION.parse::<f32>().filter(|v| *v > 0.0).unwrap_or(8.0);
        let params = MotionParams {
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            size: [w as f32, h as f32, 1.0 / w.max(1) as f32, 1.0 / h.max(1) as f32],
            vehicle_now,
            vehicle_prev,
            vehicle_box,
            vehicle_centre,
            flags: [known as u8 as f32, debug_px, 0.0, 0.0],
        };
        self.queue.write_buffer(&m.params, 0, bytemuck::bytes_of(&params));
        m.prev = Some((view_proj, ro, f.lighting.inside.map(|(o, h, _)| (o, h))));
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

//! Optional, real AMD FidelityFX FSR 3 *upscaling* through wgpu-ffx 0.1.
//!
//! This is not frame generation and not the TAA-based "FSR 2 Preview".
//! Feature-gated: no shader compilation, allocation or GPU feature request in
//! the default renderer. The only integration point is a same-frame desktop
//! color/depth/motion contract; old OpenGL devices keep their existing path.
//!
//! SPDX-License-Identifier: AGPL-3.0-only

use crate::pipelines::common::*;
use crate::*;
use wgpu_ffx::{
    FsrContext, FsrContextFlags, FsrContextInfo, FsrDispatchFlags, FsrDispatchInfo, FsrView,
    get_jitter_offset, get_jitter_phase_count,
};

struct Targets {
    render: (u32, u32),
    display: (u32, u32),
    color: wgpu::Texture,
    output: wgpu::Texture,
    dilated_depth: wgpu::Texture,
    dilated_motion: wgpu::Texture,
    reconstructed_depth: wgpu::Buffer,
    view: FsrView,
    present: wgpu::BindGroup,
}

/// Owns native wgpu 29 FidelityFX compute passes and the per-camera history.
/// Must not be run concurrently with the standalone TAA or FSR 1.0.
pub(crate) struct Fsr3 {
    context: FsrContext,
    convert: wgpu::RenderPipeline,
    convert_layout: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    frame_index: u32,
    ready: bool,
}

impl Fsr3 {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let context = FsrContext::new(FsrContextInfo {
            device: device.clone(),
            // The existing main pass is reversed-Z; scene colour is transformed
            // from the tone-mapped surface into *linear* RGBA16F first.
            flags: FsrContextFlags::DEPTH_INVERTED
                | FsrContextFlags::HIGH_DYNAMIC_RANGE
                | FsrContextFlags::AUTO_EXPOSURE
                | FsrContextFlags::MOTION_VECTORS_JITTER_CANCELLATION,
        });
        let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("FSR3 linear RGBA16F input"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fsr3_input.wgsl").into()),
        });
        let convert_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("FSR3 colour input"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }, count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("FSR3 colour input"),
            bind_group_layouts: &[Some(&convert_layout)],
            immediate_size: 0,
        });
        let convert = no_vertex_pipeline(
            "FSR3 colour input", &layout, &shader, "fs_main",
            &[target(wgpu::TextureFormat::Rgba16Float, None)],
        ).create(device);
        Self { context, convert, convert_layout, targets: None, frame_index: 0, ready: false }
    }

    pub(crate) fn reset(&mut self) {
        self.ready = false;
        self.frame_index = 0;
    }

    /// A deterministic jitter value shared by both the actual projection and
    /// the FidelityFX dispatch. Never jitter mirrors, XR eyes or the HUD.
    pub(crate) fn jitter(&self, render: (u32, u32), display: (u32, u32)) -> [f32; 2] {
        let phases = get_jitter_phase_count(render.0.max(1) as i32, display.0.max(1) as i32).max(1);
        get_jitter_offset(self.frame_index as i32, phases)
    }

    pub(crate) fn present(&self) -> Option<&wgpu::BindGroup> {
        self.ready.then(|| self.targets.as_ref().map(|t| &t.present)).flatten()
    }

    fn texture(
        device: &wgpu::Device,
        name: &'static str,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[],
        })
    }

    fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scale_layout: &wgpu::BindGroupLayout,
        scale_uniform: &wgpu::Buffer,
        scale_sampler: &wgpu::Sampler,
        render: (u32, u32),
        display: (u32, u32),
    ) -> bool {
        if render.0 < 2 || render.1 < 2 || display.0 < render.0 || display.1 < render.1 {
            return false;
        }
        if self.targets.as_ref().is_some_and(|t| t.render == render && t.display == display) {
            return true;
        }
        self.reset();
        let input_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
        let storage_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING;
        let color = Self::texture(device, "FSR3 input rgba16f", render, wgpu::TextureFormat::Rgba16Float, input_usage);
        let output = Self::texture(device, "FSR3 output rgba16f", display, wgpu::TextureFormat::Rgba16Float, storage_usage);
        let dilated_depth = Self::texture(device, "FSR3 dilated depth", render, wgpu::TextureFormat::R32Float, storage_usage);
        let dilated_motion = Self::texture(device, "FSR3 dilated motion", render, wgpu::TextureFormat::Rg16Float, storage_usage);
        let reconstructed_depth = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FSR3 reconstructed nearest depth"),
            size: u64::from(render.0) * u64::from(render.1) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = self.context.create_view(queue, [render.0, render.1], [display.0, display.1]);
        let result_view = output.create_view(&Default::default());
        let present = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FSR3 reconstructed picture"),
            layout: scale_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: scale_uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&result_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(scale_sampler) },
            ],
        });
        self.targets = Some(Targets { render, display, color, output, dilated_depth, dilated_motion, reconstructed_depth, view, present });
        true
    }

    /// Records real FSR3 compute passes into the main GPU encoder. The
    /// compositing pass afterwards draws its full-resolution output, without
    /// subsequently applying FSR1 or standalone TAA.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scale_layout: &wgpu::BindGroupLayout,
        scale_uniform: &wgpu::Buffer,
        scale_sampler: &wgpu::Sampler,
        scene_color: &wgpu::TextureView,
        depth: &wgpu::Texture,
        motion: &wgpu::Texture,
        render: (u32, u32),
        display: (u32, u32),
        camera: &Camera,
        delta_seconds: f32,
        reset: bool,
        sharpness: f32,
    ) -> bool {
        self.ready = false;
        if !self.ensure(device, queue, scale_layout, scale_uniform, scale_sampler, render, display)
            || !delta_seconds.is_finite() || delta_seconds < 0.0
        {
            return false;
        }
        let jitter = self.jitter(render, display);
        let targets = self.targets.as_mut().expect("FSR3 target allocated");
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("FSR3 colour conversion"),
            layout: &self.convert_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(scene_color),
            }],
        });
        {
            let view = targets.color.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FSR3 prepare colour"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, depth_slice: None, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
            });
            pass.set_pipeline(&self.convert);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        let dispatch = FsrDispatchInfo {
            color: targets.color.clone(),
            depth: depth.clone(),
            motion_vectors: motion.clone(),
            exposure: None,
            reactive_mask: None,
            transparency_and_composition: None,
            dilated_depth: targets.dilated_depth.clone(),
            dilated_motion_vectors: targets.dilated_motion.clone(),
            reconstructed_previous_depth: targets.reconstructed_depth.clone(),
            output: targets.output.clone(),
            jitter_offset: jitter,
            motion_vector_scale: [render.0 as f32, render.1 as f32],
            render_size: [render.0, render.1],
            upscale_size: [display.0, display.1],
            enable_sharpening: sharpness > 0.001,
            sharpness: sharpness.clamp(0.0, 1.0),
            frame_time_delta: delta_seconds.max(0.001).min(0.5) * 1000.0,
            pre_exposure: 1.0,
            reset_history: reset || self.frame_index == 0,
            camera_near: camera.near,
            camera_far: camera.far,
            camera_fov_y: camera.fov_deg.to_radians(),
            view_space_to_meters_factor: 1.0,
            flags: FsrDispatchFlags::empty(),
        };
        if let Err(e) = self.context.dispatch(&mut targets.view, encoder, &dispatch) {
            log::warn!("FSR 3 dispatch refused; restoring standard upscaler: {e}");
            self.reset();
            return false;
        }
        self.ready = true;
        self.frame_index = self.frame_index.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn conversion_shader_parses() {
        let text = include_str!("fsr3_input.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(text)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(text)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()
        ).validate(&module).expect("FSR3 colour conversion shader must validate");
    }
}

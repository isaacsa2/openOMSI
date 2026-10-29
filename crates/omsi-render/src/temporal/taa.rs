//! Optional lightweight temporal history filter, NOT AMD FSR 2.
//! This serves as a test consumer for the motion buffers and a future fallback.
//! Nothing is allocated or compiled for users who have not opted in.

use crate::*;
use crate::pipelines::common::*;

pub(crate) struct Taa {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
}

struct Targets {
    size: (u32, u32),
    _textures: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    present: [wgpu::BindGroup; 2],
    last: usize,
    ready: bool,
}

impl Taa {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("experimental TAA"),
            source: wgpu::ShaderSource::Wgsl(include_str!("taa.wgsl").into()),
        });
        let texture_entry = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("experimental TAA inputs"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
                texture_entry(1, true),
                texture_entry(2, false),
                texture_entry(3, true),
                sampler_entry(4),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("experimental TAA"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = no_vertex_pipeline("experimental TAA", &pl, &shader, "fs_taa", &[target(format, None)]).create(device);
        let params = uniform_buffer(device, "experimental TAA params", 16);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("TAA history sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, params, sampler, targets: None }
    }

    pub(crate) fn reset(&mut self) {
        if let Some(t) = &mut self.targets {
            t.ready = false;
        }
    }

    pub(crate) fn present(&self) -> Option<&wgpu::BindGroup> {
        self.targets.as_ref().filter(|t| t.ready).map(|t| &t.present[t.last])
    }
}

impl Renderer {
    /// Runs after tone mapping and before the spatial upscaler. HUD is drawn later
    /// and must never enter a temporal history buffer.
    pub(crate) fn encode_temporal_aa(&mut self, enc: &mut wgpu::CommandEncoder, f: &FrameCtx) {
        if self.temporal_aa.is_none() { return; }
        let Some((scene_color, _)) = f.scene_target.as_ref().filter(|_| f.with_overlays && f.projection.is_none() && !f.second_eye) else {
            if let Some(taa) = self.temporal_aa.as_mut() { taa.reset(); }
            return;
        };
        let Some(motion) = self.motion.as_ref().filter(|m| m.frame_ready) else {
            self.temporal_aa.as_mut().unwrap().reset();
            return;
        };
        let Some(motion_view) = motion.target.as_ref().filter(|t| t.size == (f.width, f.height)).map(|t| &t.view) else {
            self.temporal_aa.as_mut().unwrap().reset();
            return;
        };
        let size = (f.width, f.height);
        let must_resize = self.temporal_aa.as_ref().unwrap().targets.as_ref().is_none_or(|t| t.size != size);
        if must_resize {
            let textures = [0, 1].map(|_| self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("TAA history"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
            let views = textures.each_ref().map(|t| t.create_view(&Default::default()));
            let present = views.each_ref().map(|v| self.picture_group(v));
            self.temporal_aa.as_mut().unwrap().targets = Some(Targets { size, _textures: textures, views, present, last: 1, ready: false });
        }
        let taa = self.temporal_aa.as_mut().unwrap();
        let targets = taa.targets.as_mut().unwrap();
        let next = 1 - targets.last;
        let previous_valid = targets.ready && !motion.frame_reset;
        let values = [size.0 as f32, size.1 as f32, previous_valid as u8 as f32, 0.88f32];
        self.queue.write_buffer(&taa.params, 0, bytemuck::cast_slice(&values));
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TAA current/motion/previous"),
            layout: &taa.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: taa.params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(scene_color) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(motion_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&targets.views[targets.last]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&taa.sampler) },
            ],
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("experimental TAA (FSR 2 preflight)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.views[next],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&taa.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        targets.last = next;
        targets.ready = true;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn taa_shader_parses() {
        let src = include_str!("taa.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        ).validate(&module).expect("TAA shader must validate");
    }
}

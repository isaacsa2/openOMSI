//! Standalone, opt-in motion-vector pass for future NVIDIA DLSS evaluation.
//! The existing 3D, VR, mirror, antialiasing and presentation paths stay untouched.
use crate::*;

pub(crate) struct MotionTargets {
    size: (u32, u32),
    _motion: wgpu::Texture,
    pub(crate) motion_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

#[derive(Clone, Copy)]
pub(crate) struct RecordedMotion {
    pub uniform: temporal_motion::MotionUniform,
    pub jitter_px: [f32; 2],
    pub reset: bool,
}

pub(crate) struct TemporalGpu {
    last: Option<RecordedMotion>,
    history: temporal_motion::MotionHistory,
    pipelines: [wgpu::RenderPipeline; 6],
    sky: wgpu::RenderPipeline,
    bind_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    previous_models: wgpu::Buffer,
    model_capacity: u64,
    bind_group: wgpu::BindGroup,
    targets: Option<MotionTargets>,
    old_targets: std::collections::VecDeque<MotionTargets>,
}

impl TemporalGpu {
    pub(crate) fn new(device: &wgpu::Device, scene: &pipelines::scene::SceneBase) -> Self {
        // Reuse the same scene vertex functions, alpha-test functions and shader
        // bindings as the current depth prepass. Build this module only on
        // explicit opt-in; the normal renderer shader remains unchanged.
        let wgsl = temporal_motion::shader_source();
        let module = pipelines::common::compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("DLSS temporal motion"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("DLSS temporal history"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("DLSS motion meshes"),
            bind_group_layouts: &[Some(&scene.camera_layout), Some(&scene.material_layout), None, Some(&bind_layout)],
            immediate_size: 0,
        });
        // Both sky and meshes share one pipeline layout: switching pipelines
        // must not invalidate the group(3) motion-history binding.
        let color = [Some(wgpu::ColorTargetState {
            format: temporal_motion::MOTION_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL,
        })];
        let make = |kind: u8, cull: bool| {
            let entry = match kind {
                0 => "fs_temporal_motion",
                1 => "fs_temporal_motion_cutout",
                _ => "fs_temporal_motion_transmap",
            };
            pipelines::common::compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
                label: Some("DLSS motion depth/material"),
                layout: Some(&mesh_layout),
                vertex: wgpu::VertexState {
                    module: &module, entry_point: Some("vs_temporal_motion"),
                    buffers: &[scene.vertex_layout.clone()], compilation_options: Default::default(),
                },
                primitive: one_sided_primitive(cull),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(), bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module, entry_point: Some(entry), targets: &color,
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: pipeline_cache::get(device).as_ref(),
            })
        };
        let pipelines = [(0,false),(0,true),(1,false),(1,true),(2,false),(2,true)].map(|(kind,cull)| make(kind,cull));
        let sky = pipelines::common::compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some("DLSS motion sky"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &module, entry_point: Some("vs_temporal_sky"),
                buffers: &[], compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT, depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module, entry_point: Some("fs_temporal_sky"),
                targets: &color, compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: pipeline_cache::get(device).as_ref(),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DLSS motion matrices"), size: std::mem::size_of::<temporal_motion::MotionUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let initial_capacity = 64;
        let previous_models = Self::model_buffer(device, initial_capacity);
        let bind_group = Self::bind_history(device, &bind_layout, &uniform, &previous_models);
        Self {
            history: Default::default(), pipelines, sky, bind_layout,
            uniform, previous_models, model_capacity: initial_capacity,
            bind_group, targets: None, old_targets: Default::default(), last: None,
        }
    }


    pub(crate) fn next_jitter(&self) -> [f32; 2] {
        self.history.next_jitter()
    }

    pub(crate) fn last_frame(&self) -> Option<RecordedMotion> {
        self.last
    }

    pub(crate) fn inputs(&self) -> Option<(&wgpu::Texture, &wgpu::Texture)> {
        self.targets.as_ref().map(|t| (&t._depth, &t._motion))
    }

    fn model_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("DLSS previous model matrices"), size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn bind_history(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        models: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DLSS temporal history"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: models.as_entire_binding() },
            ],
        })
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) { return; }
        let texture = |label, format, usage| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format, usage, view_formats: &[],
        });
        let motion = texture("DLSS motion vectors", temporal_motion::MOTION_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth = texture("DLSS motion depth", DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let motion_view = motion.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        // Streamline may still hold native D3D12 references to the previous
        // frame's input textures during a resize; retain a few older targets
        // instead of dropping them immediately while GPU work is in flight.
        if let Some(old) = self.targets.take() {
            self.old_targets.push_back(old);
            while self.old_targets.len() > 5 { self.old_targets.pop_front(); }
        }
        self.targets = Some(MotionTargets { size, _motion: motion, motion_view, _depth: depth, depth_view });
    }

    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        camera: &Camera,
        size: (u32, u32),
        batches: &[Batch],
    ) {
        if size.0 == 0 || size.1 == 0 || scene.camera_bind_group.is_none() { return; }
        let prepared = self.history.prepare(scene, camera, size);
        self.last = Some(RecordedMotion {
            uniform: prepared.uniform, jitter_px: prepared.jitter_px, reset: prepared.reset,
        });
        let bytes: &[u8] = bytemuck::cast_slice(&prepared.previous_models);
        let required = (bytes.len() as u64).max(64);
        if required > self.model_capacity {
            self.model_capacity = required.next_power_of_two();
            self.previous_models = Self::model_buffer(device, self.model_capacity);
            self.bind_group = Self::bind_history(device, &self.bind_layout, &self.uniform, &self.previous_models);
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&prepared.uniform));
        if !bytes.is_empty() { queue.write_buffer(&self.previous_models, 0, bytes); }
        self.ensure_targets(device, size);
        let Some(targets) = self.targets.as_ref() else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("DLSS motion vectors"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.motion_view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        // The sky pipeline shares the scene's pipeline layout with meshes.
        // wgpu validates group(0) even for the fullscreen sky draw, so the
        // active camera bind group MUST be bound before its first draw.
        // Binding it only for the later mesh batches triggered a GPU validation
        // error ("DLSS motion sky expects a BindGroup at index 0").
        pass.set_bind_group(0, scene.camera_bind_group.as_ref(), &[]);
        pass.set_bind_group(3, Some(&self.bind_group), &[]);
        pass.set_pipeline(&self.sky);
        pass.draw(0..3, 0..1);
        encode_batches(&mut pass, scene, batches, |pipe| &self.pipelines[pipe as usize]);
        drop(pass);
        // This state belongs only to a *desktop main view*, never a mirror or VR eye.
        self.history.finish(scene, camera, size);
    }
}

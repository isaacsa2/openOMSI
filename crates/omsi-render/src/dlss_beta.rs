//! Opt-in DLSS SR / DLAA beta presentation path for Windows DX12.
//! Only enabled with both the Cargo feature and OMSI_DLSS_BETA=quality|... .
//! This does not redistribute NVIDIA runtime files.
use crate::*;

pub(crate) struct DlssBeta {
    pub mode: nvidia_dlss::DlssMode,
    runtime: Option<nvidia_dlss::Runtime>,
    targets: Option<Targets>,
    old_targets: std::collections::VecDeque<Targets>,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    failed: bool,
}
struct Targets {
    render: (u32, u32),
    output: (u32, u32),
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    output_tex: wgpu::Texture,
    output_view: wgpu::TextureView,
    output_group: wgpu::BindGroup,
}

impl DlssBeta {
    pub(crate) fn requested() -> nvidia_dlss::DlssMode {
        omsi_cfg::env::var("OMSI_DLSS_BETA").ok()
            .map(|v| nvidia_dlss::DlssMode::parse(&v))
            .unwrap_or_default()
    }

    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, mode: nvidia_dlss::DlssMode) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("DLSS beta output sampler"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("DLSS beta output sampler"),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("DLSS beta display"),
            source: wgpu::ShaderSource::Wgsl(include_str!("dlss_present.wgsl").into()),
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("DLSS beta display"), bind_group_layouts: &[Some(&layout)], immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("DLSS beta display"),
            layout: Some(&pipe_layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs_main"),
                buffers: &[], compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })], compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: pipeline_cache::get(device).as_ref(),
        });
        Self { mode, runtime: None, targets: None, old_targets: Default::default(), pipeline, layout, sampler, failed: false }
    }

    fn allocate(&mut self, renderer: &Renderer, render: (u32, u32), output: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.render == render && t.output == output) { return; }
        let texture = |label, size:(u32,u32), format, usage| renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width:size.0,height:size.1,depth_or_array_layers:1 },
            mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,
            format,usage,view_formats:&[],
        });
        let color = texture(
            "DLSS beta scene", render, renderer.format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
        );
        let color_view = color.create_view(&Default::default());
        let output_tex = texture(
            "DLSS beta output", output, wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING
        );
        let output_view = output_tex.create_view(&Default::default());
        let output_group = renderer.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DLSS beta full resolution"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&output_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        if let Some(old) = self.targets.take() {
            self.old_targets.push_back(old);
            while self.old_targets.len() > 5 { self.old_targets.pop_front(); }
        }
        self.targets = Some(Targets { render, output, color, color_view, output_tex, output_view, output_group });
    }

    /// Draw the low-resolution 3D frame, evaluate Streamline on the same native
    /// D3D12 queue, then composite the full-size result and native-resolution HUD.
    /// On error, this returns false and the caller renders normally.
    pub(crate) fn render(
        &mut self,
        renderer: &mut Renderer,
        scene: &mut Scene,
        swap: &wgpu::TextureView,
        width: u32, height: u32,
        camera: &Camera, lighting: &Lighting,
    ) -> bool {
        if self.failed || width == 0 || height == 0 { return false; }
        if self.runtime.is_none() {
            match nvidia_dlss::Runtime::load(&renderer.device, &renderer.queue) {
                Ok(runtime) => {
                    log::info!("NVIDIA DLSS beta initialized: {}", self.mode.as_str());
                    self.runtime = Some(runtime);
                }
                Err(e) => {
                    log::warn!("NVIDIA DLSS beta unavailable, using standard rendering: {e:#}");
                    self.failed = true;
                    return false;
                }
            }
        }
        let requested = self.mode;
        let out_size = (width,height);
        let render_size = self.runtime.as_mut().unwrap().render_size(requested,out_size);
        self.allocate(renderer,render_size,out_size);
        let Some(targets) = self.targets.as_ref() else { return false; };
        // Save and restore the player's options; the DLSS picture must not be
        // scaled by FSR1, FXAA or the ordinary render-scale pass a second time.
        let saved = renderer.options;
        renderer.options.render_scale = 1.0;
        // DLSS already selected the internal resolution; a device-specific
        // max_scene cap must not silently downscale the input a second time.
        renderer.options.max_scene = None;
        renderer.options.fsr1 = false;
        renderer.options.fxaa = false;
        // Hide overlays while the 3D picture is being drawn. HUD is rendered only
        // after Streamline, at the *actual window size*.
        let overlays = std::mem::take(&mut scene.overlays);
        let overlay_res = std::mem::take(&mut scene.overlay_res);
        renderer.dlss_frame_active = true;
        renderer.render_inner(scene,&targets.color_view,render_size.0,render_size.1,camera,lighting,true,None,None,false);
        renderer.dlss_frame_active = false;
        scene.overlays = overlays;
        scene.overlay_res = overlay_res;
        renderer.options = saved;
        let Some(motion) = renderer.temporal_gpu.as_ref() else {
            log::warn!("DLSS beta: temporal GPU resources were not initialized");
            self.failed = true;
            return false;
        };
        let (Some((depth,mvec)), Some(frame)) = (motion.inputs(), motion.last_frame()) else {
            log::warn!("DLSS beta: no motion and depth from the current desktop frame");
            self.failed = true;
            return false;
        };
        // Native Streamline expects its output to be in RENDER_TARGET state at
        // tagging time. Make this its last wgpu use before the native call.
        let mut enc = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("DLSS beta output prepare") });
        {
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("DLSS beta output state"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.output_view,depth_slice:None,resolve_target:None,
                    ops:wgpu::Operations { load:wgpu::LoadOp::Clear(wgpu::Color::BLACK),store:wgpu::StoreOp::Store },
                })],depth_stencil_attachment:None,timestamp_writes:None,
                occlusion_query_set:None,multiview_mask:None,
            });
        }
        renderer.queue.submit(Some(enc.finish()));
        let projection = Mat4::perspective_rh(camera.fov_deg.to_radians(),width as f32/height as f32,camera.far,camera.near);
        let vp = Mat4::from_cols_array_2d(&frame.uniform.current_vp);
        let prev_here = Mat4::from_cols_array_2d(&frame.uniform.previous_vp_here);
        let c = nvidia_dlss::FrameConstants {
            view_to_clip:projection.to_cols_array_2d(),
            clip_to_view:projection.inverse().to_cols_array_2d(),
            clip_to_prev_clip:(prev_here*vp.inverse()).to_cols_array_2d(),
            prev_clip_to_clip:(vp*prev_here.inverse()).to_cols_array_2d(),
            jitter:frame.jitter_px,
            camera_pos:(camera.position-scene.render_origin).as_vec3().to_array(),
            camera_up:camera.up().to_array(),
            camera_right:camera.right().to_array(),
            camera_fwd:camera.forward().to_array(),
            near:camera.near,far:camera.far,
            fov:camera.fov_deg.to_radians(),aspect:width as f32/height as f32,
            reset:frame.reset,
        };
        let inputs = nvidia_dlss::Inputs {
            color:&targets.color,depth,motion:mvec,output:&targets.output_tex,
            render_size,output_size:out_size,
        };
        if let Err(e) = self.runtime.as_mut().unwrap().evaluate(&renderer.device,&renderer.queue,requested,&inputs,&c) {
            log::error!("NVIDIA DLSS evaluation failed, disabling for this session: {e:#}");
            self.failed = true;
            return false;
        }
        // Present DLSS output before the HUD; do NOT upscale map/menus/cockpit UI.
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {label:Some("DLSS beta present")});
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:Some("DLSS output and native HUD"),
                color_attachments:&[Some(wgpu::RenderPassColorAttachment {
                    view:swap,depth_slice:None,resolve_target:None,
                    ops:wgpu::Operations {load:wgpu::LoadOp::Clear(wgpu::Color::BLACK),store:wgpu::StoreOp::Store},
                })],depth_stencil_attachment:None,timestamp_writes:None,
                occlusion_query_set:None,multiview_mask:None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0,Some(&targets.output_group),&[]);
            pass.draw(0..3,0..1);
            if !scene.overlay_res.is_empty() {
                pass.set_pipeline(&renderer.overlay_pipeline_1x);
                for (_,_,group,_) in &scene.overlay_res {
                    pass.set_bind_group(0,Some(group),&[]);
                    pass.draw(0..6,0..1);
                }
            }
        }
        renderer.queue.submit(Some(encoder.finish()));
        true
    }
}

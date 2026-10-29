//! Genuine AMD FidelityFX FSR 2.3.3, WGSL (unofficial MIT port).
//! Unlike the old "FSR 2 Preview", this calls the actual FFX FSR2 host
//! algorithm and the WGSL compute passes from sp-fidelity 0.2.0.
//! The adapter uses the application's *patched wgpu 29* types exclusively.
//! Enabled at compile-time by default, but only creates GPU passes for compatible selected adapters.

use super::fsr2_port;
use super::fsr2_port::FfxWgpuBackend;
use crate::*;
use crate::pipelines::common::*;
use sp_fidelity::fsr2::*;
use sp_fidelity::types::*;
use std::cell::RefCell;
use std::rc::Rc;

struct Targets {
    render: (u32, u32),
    display: (u32, u32),
    color: wgpu::Texture,
    output: wgpu::Texture,
    output_group: wgpu::BindGroup,
}
struct Fsr2Engine {
    backend: Rc<RefCell<FfxWgpuBackend>>,
    interface: sp_fidelity::interface::FfxInterfaceRef,
    context: FfxFsr2Context,
    convert: wgpu::RenderPipeline,
    convert_layout: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    index: u32,
    ready: bool,
    refused: bool,
}

impl Fsr2Engine {
    pub(crate) fn required_features() -> wgpu::Features {
        fsr2_port::required_features()
    }

    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let (backend, interface) = fsr2_port::ffx_get_interface_wgpu(device);
        // No second wgpu device or native SDK pointer is introduced here.
        let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("FSR2 WGSL input conversion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fsr2_input.wgsl").into()),
        });
        let convert_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("FSR2 source colour"),
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
            label: Some("FSR2 input conversion"),
            bind_group_layouts: &[Some(&convert_layout)],
            immediate_size: 0,
        });
        let convert = no_vertex_pipeline(
            "FSR2 input conversion", &layout, &shader, "fs_main",
            &[target(wgpu::TextureFormat::Rgba16Float, None)],
        ).create(device);
        Self {
            backend, interface, context: FfxFsr2Context::default(),
            convert, convert_layout, targets: None, index: 0, ready: false, refused: false,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.index = 0;
        self.ready = false;
    }
    pub(crate) fn jitter(&self, render: (u32, u32), display: (u32, u32)) -> [f32; 2] {
        let phases = ffx_fsr2_get_jitter_phase_count(render.0.max(1) as i32, display.0.max(1) as i32).max(1);
        ffx_fsr2_get_jitter_offset(self.index as i32, phases)
            .map(|(x, y)| [x, y]).unwrap_or([0.0, 0.0])
    }
    pub(crate) fn present(&self) -> Option<&wgpu::BindGroup> {
        self.ready.then(|| self.targets.as_ref().map(|t| &t.output_group)).flatten()
    }
    fn tex(device: &wgpu::Device, label: &str, size: (u32, u32), usage: wgpu::TextureUsages) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {width:size.0,height:size.1,depth_or_array_layers:1},
            mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,
            format:wgpu::TextureFormat::Rgba16Float,usage,view_formats:&[],
        })
    }
    fn ensure(
        &mut self, device:&wgpu::Device, queue:&wgpu::Queue,
        layout:&wgpu::BindGroupLayout, uniform:&wgpu::Buffer, sampler:&wgpu::Sampler,
        render:(u32,u32), display:(u32,u32),
    )->bool {
        if render.0<2 || render.1<2 || display.0<render.0 || display.1<render.1 {return false;}
        if self.targets.as_ref().is_some_and(|t|t.render==render && t.display==display){return true;}
        if self.targets.is_some() {
            let _=ffx_fsr2_context_destroy(&mut self.context);
            self.context=FfxFsr2Context::default();
        }
        self.ready=false;self.index=0;
        let desc=FfxFsr2ContextDescription {
            // Input comes from the tone-mapped (SDR) scene. Do not claim HDR.
            flags: FFX_FSR2_ENABLE_DEPTH_INVERTED | FFX_FSR2_ENABLE_AUTO_EXPOSURE
                | FFX_FSR2_ENABLE_MOTION_VECTORS_JITTER_CANCELLATION,
            max_render_size:FfxDimensions2D{width:render.0,height:render.1},
            display_size:FfxDimensions2D{width:display.0,height:display.1},
            fp_message:None,backend_interface:self.interface.clone(),
        };
        if let Err(code)=ffx_fsr2_context_create(&mut self.context,&desc){
            log::warn!("FSR2 WGSL context refused: {code:#x}");
            self.targets=None;return false;
        }
        let color=Self::tex(device,"FSR2 linear source",render,
            wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC);
        let output=Self::tex(device,"FSR2 WGSL output",display,
            wgpu::TextureUsages::STORAGE_BINDING|wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST);
        let view=output.create_view(&Default::default());
        let output_group=device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:Some("FSR2 reconstructed image"),layout,
            entries:&[
                wgpu::BindGroupEntry{binding:0,resource:uniform.as_entire_binding()},
                wgpu::BindGroupEntry{binding:1,resource:wgpu::BindingResource::TextureView(&view)},
                wgpu::BindGroupEntry{binding:2,resource:wgpu::BindingResource::Sampler(sampler)},
            ],
        });
        let _=queue;
        self.targets=Some(Targets{render,display,color,output,output_group});true
    }

    /// Reconstruct from same-frame color, reverse-Z and motion using the real
    /// FFX WGSL passes. No CPU copies. One encoder is handed to the backend
    /// and returned before the game's remaining post-processing is encoded.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &mut self, device:&wgpu::Device, queue:&wgpu::Queue,
        encoder:&mut wgpu::CommandEncoder,
        layout:&wgpu::BindGroupLayout, uniform:&wgpu::Buffer,sampler:&wgpu::Sampler,
        source:&wgpu::TextureView,depth:&wgpu::Texture,motion:&wgpu::Texture,
        render:(u32,u32),display:(u32,u32),camera:&Camera,
        dt:f32, reset:bool, sharpness:f32,
    )->bool{
        self.ready=false;
        if self.refused || !dt.is_finite() || dt<0.0 { return false; }
        // SDK context creation compiles additional shader permutations at the
        // first frame or resize: capture driver validation/OOM instead of
        // allowing an optional effect to poison the main render device.
        let setup = crate::gpu::optional_pipeline(device, "FSR2 WGSL context", || {
            self.ensure(device,queue,layout,uniform,sampler,render,display)
        });
        if setup != Some(true) {
            self.refused = true;
            return false;
        }
        let jitter=self.jitter(render,display);
        let target=self.targets.as_ref().unwrap();
        let view=target.color.create_view(&Default::default());
        let bg=device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:Some("FSR2 input color group"),layout:&self.convert_layout,
            entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(source)}],
        });
        {
            let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:Some("FSR2 prepare linear source"),
                color_attachments:&[Some(wgpu::RenderPassColorAttachment {
                    view:&view,depth_slice:None,resolve_target:None,
                    ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color::BLACK),store:wgpu::StoreOp::Store},
                })],
                depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None,
            });
            pass.set_pipeline(&self.convert);
            pass.set_bind_group(0,&bg,&[]);
            pass.draw(0..3,0..1);
        }
        let mut backend=self.backend.borrow_mut();
        let mut owned=std::mem::replace(encoder,device.create_command_encoder(&wgpu::CommandEncoderDescriptor{label:Some("FSR2 next passes")}));
        let cmd=backend.ffx_get_command_list_wgpu(owned);
        let inputs=FfxFsr2DispatchDescription {
            command_list:cmd,
            color:backend.ffx_get_resource_wgpu(&target.color,"FSR2 color",FFX_RESOURCE_STATE_COMPUTE_READ),
            depth:backend.ffx_get_resource_wgpu(depth,"FSR2 depth",FFX_RESOURCE_STATE_COMPUTE_READ),
            motion_vectors:backend.ffx_get_resource_wgpu(motion,"FSR2 motion",FFX_RESOURCE_STATE_COMPUTE_READ),
            output:backend.ffx_get_resource_wgpu(&target.output,"FSR2 output",FFX_RESOURCE_STATE_UNORDERED_ACCESS),
            jitter_offset:FfxFloatCoords2D{x:jitter[0],y:jitter[1]},
            motion_vector_scale:FfxFloatCoords2D{x:render.0 as f32,y:render.1 as f32},
            render_size:FfxDimensions2D{width:render.0,height:render.1},
            enable_sharpening:sharpness>0.001,
            sharpness:sharpness.clamp(0.0,1.0),
            frame_time_delta:dt.clamp(0.001,0.5)*1000.0,
            pre_exposure:1.0,reset:reset||self.index==0,
            camera_near:camera.near,camera_far:camera.far,
            camera_fov_angle_vertical:camera.fov_deg.to_radians(),
            view_space_to_meters_factor:1.0,
            ..Default::default()
        };
        drop(backend);
        let dispatch=ffx_fsr2_context_dispatch(&mut self.context,&inputs);
        let mut backend=self.backend.borrow_mut();
        owned=backend.ffx_take_command_list_wgpu(cmd).expect("FSR2 returned its encoder");
        *encoder=owned;
        let gpu_error=backend.take_job_error();
        drop(backend);
        if let Err(code)=dispatch{
            log::warn!("FSR2 WGSL dispatch failed: {code:#x}; backend disabled until toggled");
            self.reset();self.refused=true;return false;
        }
        if let Some(err)=gpu_error{
            log::warn!("FSR2 WGSL GPU job rejected: {err}; backend disabled until toggled");
            self.reset();self.refused=true;return false;
        }
        // A benchmark must not mistake an absent/failed FSR2 backend for an
        // actual FidelityFX result. Printed only after the first accepted FSR2
        // WGSL dispatch in this context (not a GPU image-quality guarantee).
        if self.index == 0 {
            log::info!(
                "FSR2_WGSL_BENCH_DISPATCH_OK: {}x{} -> {}x{} (WGSL)",
                render.0, render.1, display.0, display.1
            );
        }
        self.index=self.index.wrapping_add(1);
        self.ready=true;true
    }
}
impl Drop for Fsr2Engine {
    fn drop(&mut self) {
        if self.targets.is_some(){let _=ffx_fsr2_context_destroy(&mut self.context);}
    }
}

/// FSR2's upstream interface is Rc<RefCell<dyn FfxInterface>>, deliberately
/// !Send. The renderer itself must remain Send because ANGLE compiles it on
/// a managed compiler thread and transfers it back to the game thread.
/// Keep the SDK context on the thread performing actual FSR2 dispatches.
/// Only Send-safe GPU handles and a unique ID live in Renderer.
thread_local! {
    static THREAD_ENGINES: RefCell<std::collections::HashMap<u64, Fsr2Engine>> =
        RefCell::new(std::collections::HashMap::new());
}
static NEXT_ENGINE_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

pub(crate) struct Fsr2Wgsl {
    id: u64,
    index: u32,
    ready_group: Option<wgpu::BindGroup>,
    refused: bool,
}
impl Fsr2Wgsl {
    pub(crate) fn required_features() -> wgpu::Features {
        Fsr2Engine::required_features()
    }

    pub(crate) fn available(&self) -> bool {
        !self.refused
    }

    pub(crate) fn new(_device: &wgpu::Device) -> Self {
        // Do not instantiate FfxInterface here: Renderer::build may run on
        // ANGLE's separate compiler thread and would make Renderer !Send.
        Self {
            id: NEXT_ENGINE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            index: 0,
            ready_group: None,
            refused: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.ready_group = None;
        self.index = 0;
        // If the SDK context has already been created on this thread, reset
        // its temporal history too. A new thread starts with no history.
        let _ = THREAD_ENGINES.try_with(|engines| {
            if let Some(engine) = engines.borrow_mut().get_mut(&self.id) {
                engine.reset();
            }
        });
    }

    pub(crate) fn jitter(&self, render: (u32, u32), display: (u32, u32)) -> [f32; 2] {
        let phases = ffx_fsr2_get_jitter_phase_count(
            render.0.max(1) as i32, display.0.max(1) as i32,
        ).max(1);
        ffx_fsr2_get_jitter_offset(self.index as i32, phases)
            .map(|(x, y)| [x, y])
            .unwrap_or([0.0, 0.0])
    }

    pub(crate) fn present(&self) -> Option<&wgpu::BindGroup> {
        self.ready_group.as_ref()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        layout: &wgpu::BindGroupLayout, uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler, source: &wgpu::TextureView,
        depth: &wgpu::Texture, motion: &wgpu::Texture,
        render: (u32, u32), display: (u32, u32),
        camera: &Camera, dt: f32, reset: bool, sharpness: f32,
    ) -> bool {
        self.ready_group = None;
        if self.refused {
            return false;
        }
        // This closure runs on the rendering thread, not on the temporary
        // ANGLE build thread; Rc never crosses a thread boundary.
        THREAD_ENGINES.with(|engines| {
            let mut engines = engines.borrow_mut();
            let engine = engines.entry(self.id).or_insert_with(|| Fsr2Engine::new(device));
            let ok = engine.encode(
                device, queue, encoder, layout, uniform, sampler, source,
                depth, motion, render, display, camera, dt, reset, sharpness,
            );
            self.index = engine.index;
            self.refused = engine.refused;
            if ok {
                self.ready_group = engine.present().cloned();
            }
            ok
        })
    }
}
impl Drop for Fsr2Wgsl {
    fn drop(&mut self) {
        self.ready_group = None;
        // try_with is safe during thread-local teardown; SDK resources
        // are dropped on their owning thread when the renderer is released.
        let _ = THREAD_ENGINES.try_with(|engines| {
            engines.borrow_mut().remove(&self.id);
        });
    }
}

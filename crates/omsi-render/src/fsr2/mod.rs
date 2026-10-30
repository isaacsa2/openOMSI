//! AMD FidelityFX Super Resolution 2.2.1, ported to WGSL (docs/fsr2.md).
//!
//! The reference is GPUOpen-Effects/FidelityFX-FSR2 at 1680d1edd5c034f88ebbbb793d8b88f8842cf804
//! (v2.2.1): its shaders (`*.wgsl` here, each pass's notes say where this port differs in the
//! resources it uses, never in the arithmetic) and its host side (`params`, this file): the
//! same passes in the same order, on the renderer's own device, queue and command encoder.
//!
//! Only built when the player picks FSR 2 (`Renderer::set_upscaling`), and only where the
//! device has compute shaders and the limits the passes need (`supported`); a driver that
//! fails on a pipeline leaves FSR 2 out and the standard upscaler stays.

mod frame;
mod params;
mod resources;

pub use params::Quality;

use crate::pipelines::common::compile_shader;
use resources::{Image, Resources};

const COMMON: &str = include_str!("common.wgsl");
const LUMINANCE: &str = include_str!("luminance_pyramid.wgsl");

/// The passes' modules: the common part and the pass's own.
fn module(device: &wgpu::Device, label: &str, parts: &[&str]) -> wgpu::ShaderModule {
    let mut src = String::from(COMMON);
    for p in parts {
        src.push_str(p);
    }
    compile_shader(device, wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) })
}

/// The WGSL of every pass, for the tests (as `module` joins it).
#[cfg(test)]
pub(crate) fn sources() -> Vec<(&'static str, String)> {
    let join = |parts: &[&str]| parts.iter().fold(String::from(COMMON), |mut s, p| {
        s.push_str(p);
        s
    });
    vec![
        ("luminance pyramid", join(&[LUMINANCE, include_str!("luminance_pyramid_head.wgsl")])),
        ("luminance tail", join(&[LUMINANCE, include_str!("luminance_pyramid_tail.wgsl")])),
        ("reconstruct previous depth", join(&[include_str!("reconstruct.wgsl")])),
        ("depth clip", join(&[include_str!("depth_clip.wgsl")])),
        ("lock", join(&[include_str!("lock.wgsl")])),
        ("accumulate", join(&[include_str!("accumulate.wgsl")])),
        ("rcas", join(&[include_str!("rcas.wgsl")])),
    ]
}

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;

/// Whether the device can run the passes: compute shaders with 256-thread workgroups, four
/// storage textures and two storage buffers a stage, and the formats they write. Nothing is
/// assumed from a vendor or an API: an OpenGL or ANGLE device with ES 3.1 compute takes it too.
pub(crate) fn supported(device: &wgpu::Device, adapter_formats: impl Fn(wgpu::TextureFormat) -> wgpu::TextureFormatFeatures) -> Result<(), String> {
    let l = device.limits();
    if l.max_compute_invocations_per_workgroup < 256 || l.max_compute_workgroup_size_x < 256 {
        return Err("no compute shaders with 256-thread workgroups".into());
    }
    if l.max_compute_workgroup_storage_size < 16 * 16 * 4 {
        return Err("too little workgroup memory".into());
    }
    if l.max_storage_textures_per_shader_stage < 4 || l.max_storage_buffers_per_shader_stage < 2 {
        return Err(format!("{} storage textures and {} storage buffers a stage (4 and 2 needed)", l.max_storage_textures_per_shader_stage, l.max_storage_buffers_per_shader_stage));
    }
    if l.max_sampled_textures_per_shader_stage < 9 || l.max_bindings_per_bind_group < 15 {
        return Err("too few texture bindings".into());
    }
    for f in resources::STORAGE_FORMATS {
        let features = adapter_formats(f);
        if !features.allowed_usages.contains(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING) {
            return Err(format!("{f:?} cannot be written by a shader here"));
        }
    }
    Ok(())
}

/// A pass: its pipeline and its bind group's layout.
struct Pass {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None }
}

fn uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    entry(binding, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None })
}

fn storage_buffer(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    entry(binding, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None })
}

/// A texture read with `textureLoad` (`filterable` false) or sampled with the linear sampler.
fn texture(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    entry(binding, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false })
}

fn storage_texture(binding: u32, format: wgpu::TextureFormat) -> wgpu::BindGroupLayoutEntry {
    entry(binding, wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format, view_dimension: wgpu::TextureViewDimension::D2 })
}

fn sampler(binding: u32) -> wgpu::BindGroupLayoutEntry {
    entry(binding, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering))
}

fn pass(device: &wgpu::Device, label: &str, shader: &wgpu::ShaderModule, entry_point: &str, entries: &[wgpu::BindGroupLayoutEntry], hdr: bool) -> Pass {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let constants: &[(&str, f64)] = &[("HDR_COLOR_INPUT", if hdr { 1.0 } else { 0.0 })];
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions { constants, ..Default::default() },
        cache: None,
    });
    Pass { pipeline, layout }
}

/// Each pass's label, entry point and bind group layout, in dispatch order: the luminance
/// pyramid and its tail, reconstruct, depth clip, lock, accumulate (writing the output),
/// accumulate (sharpened after), RCAS.
fn layout_entries() -> [(&'static str, &'static str, Vec<wgpu::BindGroupLayoutEntry>); 8] {
    use wgpu::TextureFormat as F;
    let accumulate = |output: bool| {
        let mut e = vec![
            uniform(0), storage_buffer(1, true), texture(2, true), texture(3, false), texture(4, false), texture(5, true), texture(6, true), texture(7, true), texture(8, true), texture(9, false), sampler(10),
            storage_texture(11, F::Rgba16Float), storage_texture(12, F::Rgba16Float), storage_texture(13, F::Rgba8Unorm),
        ];
        if output {
            e.push(storage_texture(14, F::Rgba16Float));
        }
        e
    };
    [
        ("FSR2 luminance pyramid", "cs_luminance_pyramid", vec![uniform(0), uniform(1), texture(2, true), sampler(3), storage_texture(4, F::Rgba16Float), storage_texture(5, F::R32Float), storage_buffer(6, false)]),
        ("FSR2 luminance tail", "cs_luminance_tail", vec![uniform(0), uniform(1), texture(5, false), storage_buffer(6, false)]),
        ("FSR2 reconstruct previous depth", "cs_reconstruct_previous_depth", vec![
            uniform(0), texture(1, false), texture(2, false), texture(3, false), storage_buffer(4, true), storage_buffer(5, false),
            storage_texture(6, F::Rgba16Float), storage_texture(7, F::R32Float), storage_texture(8, F::R32Float),
        ]),
        ("FSR2 depth clip", "cs_depth_clip", vec![
            uniform(0), storage_buffer(1, true), texture(2, false), texture(3, false), texture(4, false), texture(5, false), texture(6, true), texture(7, false), texture(8, false), texture(9, false), storage_buffer(10, true),
            storage_texture(11, F::Rgba8Unorm), storage_texture(12, F::Rgba16Float), sampler(13),
        ]),
        ("FSR2 lock", "cs_lock", vec![uniform(0), texture(1, false), storage_texture(2, F::R32Float), storage_buffer(3, false)]),
        ("FSR2 accumulate", "cs_accumulate", accumulate(true)),
        ("FSR2 accumulate (sharpened after)", "cs_accumulate_sharpen", accumulate(false)),
        ("FSR2 RCAS", "cs_rcas", vec![uniform(0), storage_buffer(1, true), texture(2, false), storage_texture(3, F::Rgba16Float), uniform(4)]),
    ]
}

/// The frame's inputs (the reference's FfxFsr2DispatchDescription).
pub(crate) struct Inputs<'a> {
    /// The jittered picture at the render size (display-referred, linear).
    pub color: &'a wgpu::TextureView,
    /// Its depth, reversed (near 1, far 0).
    pub depth: &'a wgpu::TextureView,
    /// Last frame's UV minus this frame's, per render pixel, without the jitter.
    pub motion: &'a wgpu::TextureView,
    pub render: (u32, u32),
    pub display: (u32, u32),
    pub jitter: [f32; 2],
    pub near: f32,
    pub far: f32,
    pub fov_y: f32,
    pub frame_time_ms: f32,
    pub reset: bool,
    /// RCAS: 0 .. 1, None for no sharpening.
    pub sharpness: Option<f32>,
}

/// FSR 2's passes, their resources and the history between frames.
pub(crate) struct Fsr2 {
    luminance: Pass,
    luminance_tail: Pass,
    reconstruct: Pass,
    depth_clip: Pass,
    lock: Pass,
    accumulate: Pass,
    accumulate_sharpen: Pass,
    rcas: Pass,
    linear: wgpu::Sampler,
    cb: wgpu::Buffer,
    spd_cb: wgpu::Buffer,
    rcas_cb: wgpu::Buffer,
    /// The reference's default exposure, (0, 0): Exposure() reads it as 1.
    default_exposure: wgpu::Buffer,
    /// The reference's default reactivity: one R8 texel of 0.
    default_reactive: Image,
    pub(crate) res: Option<Resources>,
    history: params::History,
    /// The jitter sequence's index: one step a dispatched frame.
    jitter_index: i32,
    /// The jitter the frame's projection was drawn with, for its dispatch.
    pub(crate) frame_jitter: Option<[f32; 2]>,
    /// The present pass's bind group for `res.output` (made by the renderer).
    pub(crate) present: Option<wgpu::BindGroup>,
}

impl Fsr2 {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, hdr: bool) -> Fsr2 {
        use wgpu::TextureFormat as F;
        log::info!("renderer: compiling AMD FSR 2.2.1 (WGSL)");
        let head = module(device, "FSR2 luminance pyramid", &[LUMINANCE, include_str!("luminance_pyramid_head.wgsl")]);
        let tail = module(device, "FSR2 luminance tail", &[LUMINANCE, include_str!("luminance_pyramid_tail.wgsl")]);
        let reconstruct = module(device, "FSR2 reconstruct previous depth", &[include_str!("reconstruct.wgsl")]);
        let depth_clip = module(device, "FSR2 depth clip", &[include_str!("depth_clip.wgsl")]);
        let lock = module(device, "FSR2 lock", &[include_str!("lock.wgsl")]);
        let accumulate = module(device, "FSR2 accumulate", &[include_str!("accumulate.wgsl")]);
        let rcas = module(device, "FSR2 RCAS", &[include_str!("rcas.wgsl")]);
        let layouts = layout_entries();
        let p = |i: usize, shader| pass(device, layouts[i].0, shader, layouts[i].1, &layouts[i].2, hdr);
        let fsr2 = Fsr2 {
            luminance: p(0, &head),
            luminance_tail: p(1, &tail),
            reconstruct: p(2, &reconstruct),
            depth_clip: p(3, &depth_clip),
            lock: p(4, &lock),
            accumulate: p(5, &accumulate),
            accumulate_sharpen: p(6, &accumulate),
            rcas: p(7, &rcas),
            linear: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("FSR2 linear clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }),
            cb: buffer(device, "FSR2 constants", std::mem::size_of::<params::Constants>() as u64, wgpu::BufferUsages::UNIFORM),
            spd_cb: buffer(device, "FSR2 SPD constants", std::mem::size_of::<params::SpdConstants>() as u64, wgpu::BufferUsages::UNIFORM),
            rcas_cb: buffer(device, "FSR2 RCAS constants", 16, wgpu::BufferUsages::UNIFORM),
            default_exposure: buffer(device, "FSR2 default exposure", 8, wgpu::BufferUsages::STORAGE),
            default_reactive: {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("FSR2 default reactivity"),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: F::R8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(texture.as_image_copy(), &[0u8], wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: None, rows_per_image: None }, wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
                let view = texture.create_view(&Default::default());
                Image { _texture: texture, view }
            },
            res: None,
            history: params::History { first_execution: true, ..Default::default() },
            jitter_index: 0,
            frame_jitter: None,
            present: None,
        };
        queue.write_buffer(&fsr2.default_exposure, 0, bytemuck::cast_slice(&[0f32, 0f32]));
        fsr2
    }

    /// The jitter of the next dispatched frame at this render and display size (its
    /// projection is drawn with it, `temporal::jittered_view_projection`).
    pub(crate) fn next_jitter(&mut self, render: (u32, u32), display: (u32, u32)) -> [f32; 2] {
        let j = params::jitter_offset(self.jitter_index, params::jitter_phase_count(render.0, display.0));
        self.frame_jitter = Some(j);
        j
    }

    /// Forget the history: the next dispatch starts afresh (a teleport, a camera cut).
    pub(crate) fn reset(&mut self) {
        self.history.first_execution = true;
    }

    /// Run the passes of one frame into `encoder`; the upscaled picture is `res.output` after.
    pub(crate) fn dispatch(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, inputs: &Inputs, timestamps: Option<wgpu::ComputePassTimestampWrites>) {
        let max_render = (inputs.render.0, inputs.render.1);
        let fits = self.res.as_ref().is_some_and(|r| r.display == inputs.display && r.max_render.0 >= max_render.0 && r.max_render.1 >= max_render.1);
        if !fits {
            let r = Resources::new(device, max_render, inputs.display);
            log::info!("FSR 2: resources for {}x{} drawn, {}x{} shown ({:.0} MB)", max_render.0, max_render.1, inputs.display.0, inputs.display.1, r.bytes() as f64 / 1e6);
            self.res = Some(r);
            self.present = None;
            self.history.first_execution = true;
        }
        let res = self.res.as_ref().expect("made above");
        let input_dims = {
            let t = inputs.color.texture();
            (t.width(), t.height())
        };
        let jitter = self.frame_jitter.take().unwrap_or(inputs.jitter);
        let first = self.history.first_execution;
        let (constants, reset) = params::dispatch_constants(&mut self.history, inputs.render, res.max_render, inputs.display, input_dims, jitter, inputs.near, inputs.far, inputs.fov_y, inputs.frame_time_ms, inputs.reset);
        queue.write_buffer(&self.cb, 0, bytemuck::bytes_of(&constants));
        let (gx, gy, groups, mips) = params::spd_setup(inputs.render);
        let spd = params::SpdConstants { mips, num_work_groups: groups, work_group_offset: [0, 0], render_size: [inputs.render.0, inputs.render.1], _pad: [0; 2] };
        queue.write_buffer(&self.spd_cb, 0, bytemuck::bytes_of(&spd));
        if let Some(s) = inputs.sharpness {
            queue.write_buffer(&self.rcas_cb, 0, bytemuck::cast_slice(&params::rcas_config(s)));
        }

        // which half of each pair is this frame's (fsr2Dispatch's isOddFrame)
        let odd = self.history.resource_frame_index & 1 == 1;
        let (cur, prev) = if odd { (1, 0) } else { (0, 1) };
        // (the reference: the lock status and upscaled colour read from `srv`, written to `uav`;
        // the dilated motion vectors written to `cur`, last frame's read from `prev`)
        let (srv, uav) = (cur, prev);

        // the reference's clears: on its first frame, on a reset, and (this port) the new locks
        // every frame before the lock pass
        let mut clears: Vec<&wgpu::TextureView> = vec![&res.new_locks.view];
        if first {
            clears.extend([&res.lock_status[0].view, &res.lock_status[1].view, &res.prepared_input_color.view]);
        }
        if reset {
            clears.extend([&res.lock_status[srv].view, &res.internal_upscaled[srv].view, &res.luma_shading_change.view, &res.luma_mip5.view]);
            // (the reference clears the auto exposure to -1, 1e8: a fresh average next frame)
            queue.write_buffer(&res.auto_exposure, 0, bytemuck::cast_slice(&[-1f32, 1e8f32]));
        }
        for view in clears {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FSR2 clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        let groups = self.bind_groups(device, res, inputs, srv, uav, cur, prev);
        let src = (inputs.render.0.div_ceil(8), inputs.render.1.div_ceil(8));
        let dst = (inputs.display.0.div_ceil(8), inputs.display.1.div_ceil(8));
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("FSR2"), timestamp_writes: timestamps });
        let mut run = |p: &Pass, bg: &wgpu::BindGroup, x: u32, y: u32| {
            cpass.set_pipeline(&p.pipeline);
            cpass.set_bind_group(0, bg, &[]);
            cpass.dispatch_workgroups(x.max(1), y.max(1), 1);
        };
        run(&self.luminance, &groups.luminance, gx, gy);
        if mips > 6 {
            run(&self.luminance_tail, &groups.luminance_tail, 1, 1);
        }
        run(&self.reconstruct, &groups.reconstruct, src.0, src.1);
        run(&self.depth_clip, &groups.depth_clip, src.0, src.1);
        run(&self.lock, &groups.lock, src.0, src.1);
        if inputs.sharpness.is_some() {
            run(&self.accumulate_sharpen, &groups.accumulate_sharpen, dst.0, dst.1);
            run(&self.rcas, &groups.rcas, inputs.display.0.div_ceil(16), inputs.display.1.div_ceil(16));
        } else {
            run(&self.accumulate, &groups.accumulate, dst.0, dst.1);
        }
        drop(cpass);

        self.history.resource_frame_index = (self.history.resource_frame_index + 1) % 16;
        self.jitter_index = self.jitter_index.wrapping_add(1);
    }
}

fn buffer(device: &wgpu::Device, label: &str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// One frame's bind groups.
struct Groups {
    luminance: wgpu::BindGroup,
    luminance_tail: wgpu::BindGroup,
    reconstruct: wgpu::BindGroup,
    depth_clip: wgpu::BindGroup,
    lock: wgpu::BindGroup,
    accumulate: wgpu::BindGroup,
    accumulate_sharpen: wgpu::BindGroup,
    rcas: wgpu::BindGroup,
}

impl Fsr2 {
    #[allow(clippy::too_many_arguments)]
    fn bind_groups(&self, device: &wgpu::Device, res: &Resources, inputs: &Inputs, srv: usize, uav: usize, cur: usize, prev: usize) -> Groups {
        use wgpu::BindingResource as B;
        fn view(v: &wgpu::TextureView) -> wgpu::BindingResource<'_> {
            wgpu::BindingResource::TextureView(v)
        }
        let group = |p: &Pass, label: &str, entries: Vec<(u32, B)>| {
            let entries: Vec<wgpu::BindGroupEntry> = entries.into_iter().map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource }).collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &p.layout, entries: &entries })
        };
        let cb = || self.cb.as_entire_binding();
        let exposure = || self.default_exposure.as_entire_binding();
        let reactive = || view(&self.default_reactive.view);
        let accumulate_entries = |output: bool| {
            let mut e = vec![
                (0, cb()),
                (1, exposure()),
                (2, view(&res.dilated_reactive_masks.view)),
                (3, view(&res.dilated_motion_vectors[cur].view)),
                (4, view(&res.internal_upscaled[srv].view)),
                (5, view(&res.lock_status[srv].view)),
                (6, view(&res.prepared_input_color.view)),
                (7, view(&res.luma_history[srv].view)),
                (8, view(&res.luma_shading_change.view)),
                (9, view(&res.new_locks.view)),
                (10, B::Sampler(&self.linear)),
                (11, view(&res.internal_upscaled[uav].view)),
                (12, view(&res.lock_status[uav].view)),
                (13, view(&res.luma_history[uav].view)),
            ];
            if output {
                e.push((14, view(&res.output.view)));
            }
            e
        };
        Groups {
            luminance: group(&self.luminance, "FSR2 luminance pyramid", vec![
                (0, cb()), (1, self.spd_cb.as_entire_binding()), (2, view(inputs.color)), (3, B::Sampler(&self.linear)),
                (4, view(&res.luma_shading_change.view)), (5, view(&res.luma_mip5.view)), (6, res.auto_exposure.as_entire_binding()),
            ]),
            luminance_tail: group(&self.luminance_tail, "FSR2 luminance tail", vec![
                (0, cb()), (1, self.spd_cb.as_entire_binding()), (5, view(&res.luma_mip5.view)), (6, res.auto_exposure.as_entire_binding()),
            ]),
            reconstruct: group(&self.reconstruct, "FSR2 reconstruct previous depth", vec![
                (0, cb()), (1, view(inputs.motion)), (2, view(inputs.depth)), (3, view(inputs.color)), (4, exposure()),
                (5, res.reconstructed_previous_depth.as_entire_binding()),
                (6, view(&res.dilated_motion_vectors[cur].view)), (7, view(&res.dilated_depth.view)), (8, view(&res.lock_input_luma.view)),
            ]),
            depth_clip: group(&self.depth_clip, "FSR2 depth clip", vec![
                (0, cb()), (1, res.reconstructed_previous_depth.as_entire_binding()),
                (2, view(&res.dilated_motion_vectors[cur].view)), (3, view(&res.dilated_depth.view)),
                (4, reactive()), (5, reactive()), (6, view(&res.dilated_motion_vectors[prev].view)),
                (7, view(inputs.motion)), (8, view(inputs.color)), (9, view(inputs.depth)), (10, exposure()),
                (11, view(&res.dilated_reactive_masks.view)), (12, view(&res.prepared_input_color.view)), (13, B::Sampler(&self.linear)),
            ]),
            lock: group(&self.lock, "FSR2 lock", vec![
                (0, cb()), (1, view(&res.lock_input_luma.view)), (2, view(&res.new_locks.view)), (3, res.reconstructed_previous_depth.as_entire_binding()),
            ]),
            accumulate: group(&self.accumulate, "FSR2 accumulate", accumulate_entries(true)),
            accumulate_sharpen: group(&self.accumulate_sharpen, "FSR2 accumulate (sharpened after)", accumulate_entries(false)),
            rcas: group(&self.rcas, "FSR2 RCAS", vec![
                (0, cb()), (1, exposure()), (2, view(&res.internal_upscaled[uav].view)), (3, view(&res.output.view)), (4, self.rcas_cb.as_entire_binding()),
            ]),
        }
    }
}

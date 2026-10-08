//! The pieces most of the renderer's pipelines, samplers and bind group layouts share.

use crate::DEPTH_FORMAT;

/// A native compiler can abort before wgpu returns an error. Record both sides of each
/// operation so the last unfinished one identifies the module/entry point, not just "sky".
pub(crate) fn compile_shader(
    device: &wgpu::Device,
    descriptor: wgpu::ShaderModuleDescriptor<'_>,
) -> wgpu::ShaderModule {
    let thread = std::thread::current();
    let level = if crate::gl_backend() {
        log::Level::Info
    } else {
        log::Level::Debug
    };
    let label = descriptor.label.unwrap_or("unnamed");
    let started = std::time::Instant::now();
    log::log!(
        level,
        "renderer: shader begin {label}, thread {:?} ({})",
        thread.id(),
        thread.name().unwrap_or("unnamed")
    );
    let shader = device.create_shader_module(descriptor);
    log::log!(
        level,
        "renderer: shader end {label}, thread {:?}, {:.1} ms",
        thread.id(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    shader
}

pub(crate) fn compile_pipeline(
    device: &wgpu::Device,
    descriptor: &wgpu::RenderPipelineDescriptor<'_>,
) -> wgpu::RenderPipeline {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let thread = std::thread::current();
    let level = if crate::gl_backend() {
        log::Level::Info
    } else {
        log::Level::Debug
    };
    let fragment = descriptor.fragment.as_ref();
    let started = std::time::Instant::now();
    log::log!(level, "renderer: pipeline begin #{id} {}, vs {}, fs {}, {}x MSAA, constants {:?}, thread {:?} ({})",
        descriptor.label.unwrap_or("unnamed"), descriptor.vertex.entry_point.unwrap_or("auto"),
        fragment.and_then(|f| f.entry_point).unwrap_or("none"), descriptor.multisample.count,
        fragment.map(|f| f.compilation_options.constants), thread.id(), thread.name().unwrap_or("unnamed"));
    let pipeline = device.create_render_pipeline(descriptor);
    log::log!(
        level,
        "renderer: pipeline end #{id}, thread {:?}, {:.1} ms",
        thread.id(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    pipeline
}

/// A pipeline that draws without vertex buffers (a triangle over the screen, or quads the
/// vertex shader makes up): both stages from one module, triangles, none culled. By
/// default `vs_main`, no depth buffer and one sample.
pub(crate) struct NoVertexPipeline<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    vs: &'a str,
    fs: &'a str,
    targets: &'a [Option<wgpu::ColorTargetState>],
    depth_stencil: Option<wgpu::DepthStencilState>,
    samples: u32,
}

pub(crate) fn no_vertex_pipeline<'a>(
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    fs: &'a str,
    targets: &'a [Option<wgpu::ColorTargetState>],
) -> NoVertexPipeline<'a> {
    NoVertexPipeline { label, layout, module, vs: "vs_main", fs, targets, depth_stencil: None, samples: 1 }
}

impl<'a> NoVertexPipeline<'a> {
    pub(crate) fn vs(self, vs: &'a str) -> Self {
        NoVertexPipeline { vs, ..self }
    }

    pub(crate) fn depth(self, depth_stencil: wgpu::DepthStencilState) -> Self {
        NoVertexPipeline { depth_stencil: Some(depth_stencil), ..self }
    }

    pub(crate) fn samples(self, samples: u32) -> Self {
        NoVertexPipeline { samples, ..self }
    }

    pub(crate) fn create(self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        compile_pipeline(device, &wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(self.layout),
            vertex: wgpu::VertexState {
                module: self.module,
                entry_point: Some(self.vs),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: self.depth_stencil,
            multisample: wgpu::MultisampleState {
                count: self.samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: self.module,
                entry_point: Some(self.fs),
                targets: self.targets,
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    }
}

/// The depth test of something drawn over the scene that writes no depth of its own.
pub(crate) fn depth_test(compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(compare),
        stencil: Default::default(),
        bias: Default::default(),
    }
}

/// One colour target, every channel written.
pub(crate) fn target(format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })
}

/// A material texture sampler: linear, mip-mapped, anisotropic, `address` on all axes.
pub(crate) fn texture_sampler(device: &wgpu::Device, address: wgpu::AddressMode, anisotropy: u16) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: anisotropy,
        ..Default::default()
    })
}

/// A linear sampler clamped to the edge, without mip maps.
pub(crate) fn clamped_linear_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// A uniform buffer the CPU writes.
pub(crate) fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// A texture the fragment shader reads.
pub(crate) fn texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type, view_dimension, multisampled: false },
        count: None,
    }
}

/// A filterable 2D float texture the fragment shader reads.
pub(crate) fn float_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    texture_entry(binding, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2)
}

/// A filtering sampler the fragment shader uses.
pub(crate) fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

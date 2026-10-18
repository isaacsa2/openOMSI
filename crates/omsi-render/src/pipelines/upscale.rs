//! The render scale: the smaller 3D picture scaled up to the window.

use super::common::*;

pub(crate) struct Upscale {
    pub upscale_pipeline: wgpu::RenderPipeline,
    pub fsr_easu_pipeline: Option<wgpu::RenderPipeline>,
    pub fsr_rcas_pipeline: Option<wgpu::RenderPipeline>,
    pub copy_pipeline: wgpu::RenderPipeline,
    pub panel_pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
    pub buf: wgpu::Buffer,
}

pub(crate) fn build(device: &wgpu::Device, format: wgpu::TextureFormat, use_fsr1: bool) -> Upscale {
    // --- render scale: the smaller 3D picture scaled up to the window
    log::info!("renderer: compiling the upscaler shaders");
    let upscale_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("upscale"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../upscale.wgsl").into()),
    });
    let upscale_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("upscale"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
            float_texture_entry(1),
            sampler_entry(2),
        ],
    });
    let upscale_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("upscale"),
        bind_group_layouts: &[Some(&upscale_layout)],
        immediate_size: 0,
    });
    let targets = [target(format, None)];
    let upscale_pipeline_for = |entry| no_vertex_pipeline("upscale", &upscale_pl, &upscale_shader, entry, &targets).create(device);
    let upscale_pipeline = upscale_pipeline_for("fs_main");
    let copy_pipeline = upscale_pipeline_for("fs_copy");
    // Keep the experimental shader off the default code path: some mobile/GL drivers
    // fail during shader compilation, even if a compiled pipeline is never drawn.
    let (fsr_easu_pipeline, fsr_rcas_pipeline) = if use_fsr1 {
        let fsr_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("fsr1 EASU/RCAS"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::fsr1_source(format).into()),
        });
        let build_fsr = |entry| no_vertex_pipeline("fsr1", &upscale_pl, &fsr_shader, entry, &targets).create(device);
        (Some(build_fsr("fs_easu")), Some(build_fsr("fs_rcas")))
    } else {
        (None, None)
    };
    let panel_pipeline = upscale_pipeline_for("fs_panel");
    let upscale_buf = uniform_buffer(device, "upscale params", 16);
    Upscale { upscale_pipeline, fsr_easu_pipeline, fsr_rcas_pipeline, copy_pipeline, panel_pipeline, layout: upscale_layout, buf: upscale_buf }
}

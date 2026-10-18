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
    let (fsr_easu_pipeline, fsr_rcas_pipeline) = match use_fsr1.then(|| build_fsr(device, format, &upscale_layout)).flatten() {
        Some((easu, rcas)) => (Some(easu), Some(rcas)),
        None => (None, None),
    };
    let panel_pipeline = upscale_pipeline_for("fs_panel");
    let upscale_buf = uniform_buffer(device, "upscale params", 16);
    Upscale { upscale_pipeline, fsr_easu_pipeline, fsr_rcas_pipeline, copy_pipeline, panel_pipeline, layout: upscale_layout, buf: upscale_buf }
}

/// FSR 1's two passes (EASU, RCAS) for pictures of `format`, built when the player first
/// asks for them - at the start, or later from the menu. None when the driver refuses
/// them (a validation error, caught here rather than losing the device): the standard
/// upscaler stays, and the log says why.
pub(crate) fn build_fsr(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::BindGroupLayout) -> Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)> {
    log::info!("renderer: compiling the FSR 1 shaders");
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fsr1"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    let shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("fsr1 EASU/RCAS"),
        source: wgpu::ShaderSource::Wgsl(crate::shaders::fsr1_source(format).into()),
    });
    let targets = [target(format, None)];
    let pass = |entry| no_vertex_pipeline("fsr1", &pl, &shader, entry, &targets).create(device);
    let (easu, rcas) = (pass("fs_easu"), pass("fs_rcas"));
    match pollster::block_on(scope.pop()) {
        None => Some((easu, rcas)),
        Some(e) => {
            log::warn!("renderer: FSR 1 is not available on this driver, the standard upscaler stays: {e}");
            None
        }
    }
}

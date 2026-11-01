//! The DLSS beta's frame: the HUD drawn at the window's size after a smaller 3D picture.

use super::*;

/// Reproduces the DLSS/DLAA frame path: the low-resolution scene suppresses
/// all HUD overlays and begin_frame truncates their GPU resources. A final
/// native-resolution presentation must rebuild them before it draws Alt/HUD.
#[test]
fn restored_desktop_overlays_rebuild_for_final_window_resolution() {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::NOOP;
    descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
    let instance = wgpu::Instance::new(descriptor);
    let mut renderer = pollster::block_on(Renderer::new_with(
        &instance, None, Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, shadow_size: 256, preview_only: true, no_enhanced: true, ..Default::default() },
    )).expect("test renderer");
    let mut scene = renderer.new_scene();
    let texture = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 2, height: 2, rgba: [255u8, 255, 255, 255].repeat(4), has_alpha: false,
        },
        false,
    );
    scene.overlays.push((texture, [40.0, 30.0, 140.0, 90.0]));
    renderer.prepare_overlays(&mut scene, 1920, 1080);
    assert_eq!(scene.overlay_res.len(), 1);
    let saved_overlays = std::mem::take(&mut scene.overlays);
    let saved_resources = std::mem::take(&mut scene.overlay_res);
    // Inside the DLSS 3D frame, no HUD is drawn and resources become empty.
    renderer.prepare_overlays(&mut scene, 1114, 626);
    assert!(scene.overlay_res.is_empty());
    scene.overlays = saved_overlays;
    scene.overlay_res = saved_resources;
    renderer.prepare_overlays(&mut scene, 1920, 1080);
    assert_eq!(scene.overlay_res.len(), scene.overlays.len());
    let ndc = scene.overlay_res[0].3;
    assert!((ndc[0] - (40.0 / 1920.0 * 2.0 - 1.0)).abs() < 1e-6);
    assert!((ndc[1] - (1.0 - 30.0 / 1080.0 * 2.0)).abs() < 1e-6);
}

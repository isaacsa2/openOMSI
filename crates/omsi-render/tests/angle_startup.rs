//! Real ANGLE/D3D11 shader compilation on Windows' WARP software device. Ignored in the
//! workspace suite because it requires the packaged x64 DLLs; release.yml runs it after
//! downloading and checking them. WARP does not reproduce a particular GPU's driver.

#![cfg(windows)]

#[test]
#[ignore = "requires Windows and the packaged ANGLE DLLs"]
fn angle_d3d11_compiles_and_draws_on_warp() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    let directory = std::env::var("ANGLE_TEST_LIBRARY_DIRECTORY").expect("ANGLE DLL directory");
    let directory = std::fs::canonicalize(directory).expect("ANGLE DLL directory exists");
    for dll in ["libEGL.dll", "libGLESv2.dll"] {
        assert!(directory.join(dll).is_file(), "missing {dll}");
    }
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::GL;
    descriptor.backend_options.gl.platform = wgpu::GlPlatform::Angle;
    descriptor.backend_options.gl.angle.device_type = wgpu::AngleDeviceType::Warp;
    descriptor.backend_options.gl.angle.library_directory =
        Some(directory.to_string_lossy().into_owned());
    descriptor.backend_options.gl.context_lock_timeout = Some(std::time::Duration::from_secs(30));
    let instance = wgpu::Instance::new(descriptor);
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::GL));
    assert!(!adapters.is_empty(), "ANGLE/WARP did not expose an adapter");
    for adapter in &adapters {
        let info = adapter.get_info();
        eprintln!("ANGLE WARP adapter: {info:?}");
        assert_eq!(info.backend, wgpu::Backend::Gl);
        assert!(info.name.contains("ANGLE"), "expected ANGLE: {info:?}");
        assert_eq!(info.device_type, wgpu::DeviceType::Cpu, "expected WARP");
    }
    for basic in [false, true] {
        if basic {
            std::env::set_var("OMSI_BASIC_PIPELINES", "1");
        } else {
            std::env::remove_var("OMSI_BASIC_PIPELINES");
        }
        for no_enhanced in [true, false] {
            eprintln!("ANGLE WARP: basic={basic}, no_enhanced={no_enhanced}");
            let mut renderer = pollster::block_on(omsi_render::Renderer::new_with(
                &instance,
                None,
                Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                omsi_render::RenderOptions {
                    msaa: 1,
                    shadow_size: 256,
                    no_enhanced,
                    ..Default::default()
                },
            ))
            .expect("ANGLE renderer");
            if !basic {
                assert!(
                    !omsi_render::basic_pipelines(),
                    "full pipeline set silently fell back to basic"
                );
            }
            let scope = renderer
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let mut scene = renderer.new_scene();
            let camera = omsi_render::Camera {
                position: glam::DVec3::new(0.0, -35.0, 30.0),
                yaw: 0.0,
                pitch: -40.0,
                roll: 0.0,
                fov_deg: 60.0,
                near: 0.1,
                far: 1000.0,
            };
            let pixels = renderer
                .render_to_image(
                    &mut scene,
                    16,
                    16,
                    &camera,
                    &omsi_render::Lighting::default(),
                )
                .expect("ANGLE first frame and readback");
            assert_eq!(pixels.len(), 16 * 16 * 4);
            assert!(pollster::block_on(scope.pop()).is_none());
            assert!(renderer.device_lost().is_none());
        }
    }
}

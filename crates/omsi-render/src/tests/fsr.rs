use super::*;

/// A road with thin red rails on it seen from the cab: fine lines and hard edges, what an
/// upscaler shows its difference on.
fn rails_scene(renderer: &mut Renderer) -> Scene {
    let mut scene = renderer.new_scene();
    let strip = |half: f32, z: f32, stations: usize| {
        let mut m = MeshData { ranges: vec![(0, 6 * stations as u32, 0)], ..Default::default() };
        for i in 0..=stations {
            let y = 2.0 + 30.0 * i as f32 / stations as f32;
            m.positions.extend([Vec3::new(-half, y, z), Vec3::new(half, y, z)]);
            m.normals.extend([Vec3::Z; 2]);
            m.uvs.extend([glam::Vec2::ZERO; 2]);
        }
        for i in 0..stations as u32 {
            let a = 2 * i;
            m.indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        m
    };
    let road = renderer.add_mesh(&mut scene, &strip(3.0, 0.0, 1));
    let rails = renderer.add_mesh(&mut scene, &strip(0.15, 0.001, 10));
    let grey = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for (mesh, mat) in [(road, grey), (rails, red)] {
        let i = renderer.add_surface_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![mat]);
        scene.instances[i].render_phase = RenderPhase::Spline;
    }
    scene
}

fn renderer(render_scale: f32, fsr1: bool) -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale, fsr1, ..Default::default() },
    ))
    .expect("test renderer")
}

fn camera() -> Camera {
    Camera { position: DVec3::new(0.0, 0.0, 2.5), yaw: 0.0, pitch: -15.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 200.0 }
}

/// How far apart two pictures are, on average per channel.
fn difference(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

/// FSR 1 turned on while the game runs (the menu's way): it is built then, upscales the
/// picture - close to the standard upscaler's, not the same - and turned off again gives
/// the standard picture back.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_turned_on_while_running_upscales_the_picture() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.5, false);
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    assert!(!r.fsr_active(w, h));
    let standard = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(r.set_upscaler(true, 0.3, 0.5), "FSR 1 built");
    assert!(r.fsr_active(w, h));
    let fsr = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    let d = difference(&standard, &fsr);
    assert!(d > 0.05 && d < 12.0, "FSR 1's picture differs from the standard one a little: {d}");
    // the rails are still red in it
    let reds = fsr.chunks(4).filter(|p| p[0] > 120 && p[1] < 60 && p[2] < 60).count();
    assert!(reds > 50, "{reds} red pixels");
    assert!(r.set_upscaler(false, 0.3, 0.5));
    assert!(!r.fsr_active(w, h));
    let again = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(difference(&standard, &again) < 0.01, "the standard picture back");
}

/// FSR 1 asked for at the start, and the automatic render scale (0): at full size nothing
/// is upscaled, so FSR stays out of the way; the governor's smaller step goes through it.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_follows_the_automatic_render_scale() {
    let (w, h) = (192, 128);
    let mut r = renderer(0.0, true);
    assert!(!r.fsr_active(w, h), "full size: nothing to upscale");
    r.set_dynamic_scale(0.7);
    assert!(r.fsr_active(w, h), "the governor's step is upscaled by FSR 1");
    let mut scene = rails_scene(&mut r);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let picture = r.render_to_image(&mut scene, w, h, &camera(), &lighting).unwrap();
    assert!(picture.chunks(4).filter(|p| p[0] > 120 && p[1] < 60).count() > 50);
}

/// The three panels of a triple screen each go through FSR 1, as the window does.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr1_upscales_each_triple_screen_panel() {
    let (w, h) = (288, 96);
    let rig = TripleScreen { enabled: true, ..Default::default() };
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let mut standard = renderer(0.5, false);
    let mut scene = rails_scene(&mut standard);
    let a = standard.render_triple_to_image(&mut scene, w, h, &camera(), &lighting, &rig).unwrap();
    let mut fsr = renderer(0.5, true);
    let mut scene = rails_scene(&mut fsr);
    let b = fsr.render_triple_to_image(&mut scene, w, h, &camera(), &lighting, &rig).unwrap();
    // (the centre panel looks down the road: its rails are there in both)
    let reds = |img: &[u8]| (0..h as usize).flat_map(|y| (w as usize / 3..2 * w as usize / 3).map(move |x| (y, x))).filter(|&(y, x)| { let p = &img[(y * w as usize + x) * 4..]; p[0] > 120 && p[1] < 60 }).count();
    assert!(reds(&a) > 20 && reds(&b) > 20, "{} {}", reds(&a), reds(&b));
    let d = difference(&a, &b);
    assert!(d > 0.05 && d < 12.0, "the panels went through FSR 1: {d}");
}

/// VR with FSR 1: the headset image is full-sized, each eye is drawn at the eye resolution
/// and FSR 1 scales it up to the image; without FSR 1 the image itself is that small, as
/// before.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_vr_eye_with_fsr1_is_drawn_smaller_and_scaled_up() {
    let (w, h) = (192, 128);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let cam = camera();
    let projection = Mat4::perspective_rh(cam.fov_deg.to_radians(), w as f32 / h as f32, cam.far, cam.near);
    let eye = |r: &mut Renderer| {
        let mut scene = rails_scene(r);
        let texture = r.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("test eye"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: r.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        r.render_xr_eye(&mut scene, &texture.create_view(&Default::default()), w, h, &cam, &lighting, projection, false);
        r.read_texture(&texture, wgpu::TextureAspect::All).unwrap()
    };
    let mut plain = renderer(1.0, false);
    assert_eq!(plain.start_xr(0.65), 0.65, "without FSR 1 the headset image is the eye resolution");
    let full = eye(&mut plain);
    let mut fsr = renderer(1.0, true);
    assert_eq!(fsr.start_xr(0.65), 1.0, "with FSR 1 the headset image is full-sized");
    assert_eq!(fsr.xr_fsr_scale(), Some(0.65));
    let scaled = eye(&mut fsr);
    assert!(scaled.chunks(4).filter(|p| p[0] > 120 && p[1] < 60).count() > 50, "the rails in the eye's picture");
    let d = difference(&full, &scaled);
    assert!(d > 0.05 && d < 12.0, "drawn at 65 % and scaled up by FSR 1: {d}");
    // FSR 1 turned off later: the eye stays small, scaled up by the standard upscaler
    assert!(fsr.set_upscaler(false, 0.3, 1.0));
    assert_eq!(fsr.xr_fsr_scale(), Some(0.65));
    let standard = eye(&mut fsr);
    assert!(difference(&scaled, &standard) > 0.05);
}

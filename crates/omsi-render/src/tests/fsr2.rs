use super::*;

// AMD FSR 2 (WGSL, fsr2/): the window's picture drawn smaller with a jitter and resolved over
// the frames to the window's size.

fn renderer(render_scale: f32) -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale, ..Default::default() },
    ))
    .expect("test renderer")
}

/// Thin red rails and white posts on a grey road, the posts' edges sloping: what a smaller
/// picture loses and the jitter brings back.
fn scene(r: &mut Renderer) -> Scene {
    let mut scene = r.new_scene();
    let quad = |c: [Vec3; 4], n: Vec3| MeshData { positions: c.to_vec(), normals: vec![n; 4], uvs: vec![glam::Vec2::ZERO; 4], ranges: vec![(0, 6, 0)], indices: vec![0, 1, 2, 0, 2, 3], one_sided: false };
    let flat = |x0: f32, x1: f32, y0: f32, y1: f32, z: f32| quad([Vec3::new(x0, y0, z), Vec3::new(x1, y0, z), Vec3::new(x1, y1, z), Vec3::new(x0, y1, z)], Vec3::Z);
    let grey = r.add_material(&mut scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    let red = r.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let white = r.add_material(&mut scene, None, AlphaMode::Opaque, [0.9, 0.9, 0.9, 1.0], true);
    let road = r.add_mesh(&mut scene, &flat(-4.0, 4.0, 2.0, 80.0, 0.0));
    r.add_instance(&mut scene, road, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    for x in [-2.5f32, -1.5, -0.5, 0.5, 1.5, 2.5] {
        let rail = r.add_mesh(&mut scene, &flat(x - 0.04, x + 0.04, 2.0, 80.0, 0.01));
        r.add_instance(&mut scene, rail, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    }
    for k in 0..12 {
        let y = 6.0 + 6.0 * k as f32;
        let x = if k % 2 == 0 { -3.5 } else { 3.5 };
        let post = r.add_mesh(&mut scene, &quad([Vec3::new(x - 0.05, y, 0.0), Vec3::new(x + 0.05, y + 0.3, 0.0), Vec3::new(x + 0.05, y + 0.3, 3.0), Vec3::new(x - 0.05, y, 3.0)], -Vec3::Y));
        r.add_instance(&mut scene, post, DVec3::ZERO, Mat4::IDENTITY, vec![white]);
    }
    scene
}

fn camera() -> Camera {
    Camera { position: DVec3::new(0.3, 0.0, 2.2), yaw: 4.0, pitch: -10.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 300.0 }
}

fn light() -> Lighting {
    Lighting { shadows: false, fog_density: 0.0, ..Default::default() }
}

fn difference(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

fn fsr2(r: &mut Renderer, scale: f32) -> bool {
    r.set_upscaling(UpscalerAsk { fsr2: true, render_scale: scale, fsr2_sharpness: 0.0, ..UpscalerAsk::default() })
}

/// A still picture: FSR 2 runs every frame, gives a finite, lit picture at the window's size,
/// and over the jitter's frames comes closer to the full-size picture than the standard
/// upscaler does from the same smaller one.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr2_resolves_a_still_picture_towards_the_full_size_one() {
    let (w, h) = (240, 160);
    let scale = 1.0 / 1.5;
    let mut native = renderer(1.0);
    let mut s = scene(&mut native);
    let full = native.render_to_image(&mut s, w, h, &camera(), &light()).unwrap();

    let mut standard = renderer(scale);
    let mut s = scene(&mut standard);
    let scaled = standard.render_to_image(&mut s, w, h, &camera(), &light()).unwrap();

    let mut r = renderer(scale);
    assert!(fsr2(&mut r, scale), "FSR 2 built on this device");
    let mut s = scene(&mut r);
    let mut picture = Vec::new();
    for _ in 0..40 {
        picture = r.render_to_image(&mut s, w, h, &camera(), &light()).unwrap();
        assert!(r.fsr2_drawn.get(), "FSR 2 resolved the frame");
    }
    let lit = picture.chunks(4).filter(|p| p[0] > 20 || p[1] > 20 || p[2] > 20).count();
    assert!(lit > (w * h) as usize / 2, "{lit} lit pixels");
    let (d_fsr2, d_standard) = (difference(&picture, &full), difference(&scaled, &full));
    eprintln!("mean difference from the full-size picture: FSR 2 {d_fsr2:.3}, standard {d_standard:.3}");
    assert!(d_fsr2 < d_standard, "FSR 2 {d_fsr2:.3} vs standard {d_standard:.3}");
}

/// Turned off again the standard upscaler presents the picture as before; a different window
/// size makes FSR 2's resources again (its history starts afresh), without failing.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr2_turned_off_and_resized() {
    let scale = 0.5;
    let mut r = renderer(scale);
    let mut s = scene(&mut r);
    let before = r.render_to_image(&mut s, 160, 96, &camera(), &light()).unwrap();
    assert!(fsr2(&mut r, scale));
    for (w, h) in [(160, 96), (160, 96), (200, 120), (200, 120)] {
        r.render_to_image(&mut s, w, h, &camera(), &light()).unwrap();
        assert!(r.fsr2_drawn.get());
        assert_eq!(r.fsr2.as_ref().and_then(|f| f.res.as_ref()).map(|res| res.display), Some((w, h)));
    }
    assert!(r.set_upscaling(UpscalerAsk { render_scale: scale, ..UpscalerAsk::default() }));
    assert!(r.fsr2.is_none());
    let after = r.render_to_image(&mut s, 160, 96, &camera(), &light()).unwrap();
    assert!(!r.fsr2_drawn.get());
    assert!(difference(&before, &after) < 0.01, "the standard picture back: {}", difference(&before, &after));
}

/// A wet road with puddles: the main pass draws into a depth of its own there, but the
/// prepass's (the opaque surfaces) still gives the motion vectors, so FSR 2 resolves the
/// frame instead of leaving it to the standard upscaler.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn fsr2_resolves_wet_roads_with_puddles() {
    let scale = 1.0 / 1.5;
    let mut r = renderer(scale);
    let mut s = scene(&mut r);
    for m in s.materials.iter_mut() {
        m.uniform.params2[2] = 1.0;
    }
    let wet = Lighting { wetness: 1.0, puddles: 2, ..light() };
    assert!(r.puddles.is_some(), "puddle reflections on this device");
    assert!(fsr2(&mut r, scale));
    for _ in 0..6 {
        r.render_to_image(&mut s, 240, 160, &camera(), &wet).unwrap();
        assert!(r.fsr2_drawn.get(), "FSR 2 resolved the wet frame");
    }
    assert!(r.targets.hdr_targets.values().any(|h| h.puddles.is_some()), "the puddles were drawn");
}

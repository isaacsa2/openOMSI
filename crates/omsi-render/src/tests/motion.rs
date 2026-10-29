use super::*;

// The motion vectors (passes/motion.rs): from two pictures, where each pixel of the second was
// in the first.

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut r = pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer");
    r.set_motion_vectors(true);
    r
}

/// A quad of four corners in order.
fn quad(corners: [Vec3; 4], normal: Vec3) -> MeshData {
    MeshData {
        positions: corners.to_vec(),
        normals: vec![normal; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    }
}

/// A level rectangle at height `z`.
fn flat(x0: f32, x1: f32, y0: f32, y1: f32, z: f32) -> MeshData {
    quad([Vec3::new(x0, y0, z), Vec3::new(x1, y0, z), Vec3::new(x1, y1, z), Vec3::new(x0, y1, z)], Vec3::Z)
}

/// A grey road with red rails along it (fine red lines on grey: where a pixel came from shows).
fn road(r: &mut Renderer, scene: &mut Scene) {
    let road = r.add_mesh(scene, &flat(-3.0, 3.0, 2.0, 60.0, 0.0));
    let grey = r.add_material(scene, None, AlphaMode::Opaque, [0.3, 0.3, 0.3, 1.0], true);
    r.add_instance(scene, road, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    let red = r.add_material(scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    for x in [-1.5f32, -0.5, 0.5, 1.5] {
        let rail = r.add_mesh(scene, &flat(x - 0.08, x + 0.08, 2.0, 60.0, 0.01));
        r.add_instance(scene, rail, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    }
}

fn camera(x: f64, y: f64, yaw: f32) -> Camera {
    Camera { position: DVec3::new(x, y, 2.5), yaw, pitch: -12.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 200.0 }
}

fn light() -> Lighting {
    Lighting { shadows: false, fog_density: 0.0, ..Default::default() }
}

/// The player's bus (2.5 x 12 x 3.5 m) with its origin at `o` turned to `heading` (deg), its
/// body turned as `omsi-sim`'s `body_rotation` does (and pitched and banked by `tilt`, rad).
fn bus_at(o: DVec3, heading: f64, tilt: (f32, f32)) -> Lighting {
    let rot = Mat4::from_rotation_z((-heading).to_radians() as f32) * Mat4::from_rotation_x(tilt.0) * Mat4::from_rotation_y(tilt.1);
    Lighting { inside: Some((o, heading, [2.5, 12.0, 3.5, 0.0, 0.0, 1.75])), inside_rotation: Some(rot), ..light() }
}

fn red(img: &[u8], w: u32, x: u32, y: u32) -> bool {
    let p = &img[((y * w + x) * 4) as usize..];
    p[0] > 120 && p[1] < 70 && p[2] < 70
}

/// A camera that stays where it was: nothing moved.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_still_camera_has_no_motion() {
    let (w, h) = (160, 96);
    let mut r = renderer();
    let mut scene = r.new_scene();
    road(&mut r, &mut scene);
    for _ in 0..2 {
        r.render_to_image(&mut scene, w, h, &camera(0.0, 0.0, 0.0), &light()).unwrap();
    }
    let (mw, mh, mv) = r.read_motion().expect("motion vectors drawn");
    assert_eq!((mw, mh), (w, h));
    let most = mv.iter().map(|m| m[0].abs().max(m[1].abs())).fold(0.0f32, f32::max);
    assert!(most < 1e-4, "{most}");
}

/// The camera stepped sideways and turned: every pixel of a rail, followed back along its
/// vector, lands on a rail in the picture before (within a pixel: a rail is two or three
/// wide, and which of its edge pixels are drawn changes from one picture to the other) -
/// where staying put, without the vector, mostly misses - and the sky moved by the turn alone.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_motion_leads_back_to_where_a_point_was() {
    let (w, h) = (192, 128);
    let mut r = renderer();
    let mut scene = r.new_scene();
    road(&mut r, &mut scene);
    let before = r.render_to_image(&mut scene, w, h, &camera(0.0, 0.0, 0.0), &light()).unwrap();
    let after = r.render_to_image(&mut scene, w, h, &camera(0.25, 0.6, 2.0), &light()).unwrap();
    let (_, _, mv) = r.read_motion().expect("motion vectors drawn");
    let near_rail = |x: f32, y: f32| {
        let (x, y) = (x.floor() as i64, y.floor() as i64);
        (-1..=1).any(|dy| (-1..=1).any(|dx| {
            let (px, py) = (x + dx, y + dy);
            px >= 0 && py >= 0 && px < w as i64 && py < h as i64 && red(&before, w, px as u32, py as u32)
        }))
    };
    let (mut rails, mut back_on_rail, mut moved, mut stayed_on_rail) = (0, 0, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if !red(&after, w, x, y) {
                continue;
            }
            rails += 1;
            let m = mv[(y * w + x) as usize];
            if m[0].abs() * w as f32 > 0.5 || m[1].abs() * h as f32 > 0.5 {
                moved += 1;
            }
            if near_rail(x as f32 + 0.5 + m[0] * w as f32, y as f32 + 0.5 + m[1] * h as f32) {
                back_on_rail += 1;
            }
            if near_rail(x as f32 + 0.5, y as f32 + 0.5) {
                stayed_on_rail += 1;
            }
        }
    }
    assert!(rails > 100, "{rails} rail pixels");
    assert!(moved * 10 > rails * 9, "{moved} of {rails} rail pixels moved");
    assert!(back_on_rail * 100 > rails * 99, "{back_on_rail} of {rails} rail pixels led back onto a rail");
    assert!(stayed_on_rail * 100 < rails * 60, "without the vectors {stayed_on_rail} of {rails} would have: the test tells");
    // (the top row is sky: the 2 degree turn moves it across by about tan 2 deg over the
    // tangent of half the horizontal field of view - 60 deg high, 3:2 - of half the width,
    // a little more up there than in the middle; hardly up or down)
    let sky = mv[(w / 2) as usize];
    let tan_half = (30f32.to_radians().tan() * w as f32 / h as f32).max(1e-3);
    let across = 2f32.to_radians().tan() / tan_half * w as f32 * 0.5;
    let (sx, sy) = (sky[0].abs() * w as f32, sky[1].abs() * h as f32);
    assert!(sx > across * 0.9 && sx < across * 1.3 && sy < 0.5, "sky moved {sx} x {sy} px, the turn {across} px");
}

/// Inside the player's vehicle the cab moves with the camera: a part of it ahead of the eye
/// keeps its place on the screen (no motion), the road outside the vehicle does not.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_cab_moves_with_the_vehicle() {
    let (w, h) = (192, 128);
    let mut r = renderer();
    let mut scene = r.new_scene();
    road(&mut r, &mut scene);
    // the dashboard: a yellow panel 1.2 m ahead of the eye, low in the picture
    let panel = r.add_mesh(&mut scene, &quad([Vec3::new(-0.6, 1.2, 1.6), Vec3::new(0.6, 1.2, 1.6), Vec3::new(0.6, 1.2, 2.1), Vec3::new(-0.6, 1.2, 2.1)], -Vec3::Y));
    let yellow = r.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 1.0, 0.0, 1.0], true);
    let dash = r.add_instance(&mut scene, panel, DVec3::ZERO, Mat4::IDENTITY, vec![yellow]);
    // a bus 2.5 x 12 x 3.5 m, its origin under the eye
    let bus = |y: f64| bus_at(DVec3::new(0.0, y, 0.0), 0.0, (0.0, 0.0));
    r.render_to_image(&mut scene, w, h, &camera(0.0, 0.0, 0.0), &bus(0.0)).unwrap();
    r.set_transform(&mut scene, dash, DVec3::new(0.0, 1.5, 0.0), Mat4::IDENTITY);
    let after = r.render_to_image(&mut scene, w, h, &camera(0.0, 1.5, 0.0), &bus(1.5)).unwrap();
    let (_, _, mv) = r.read_motion().expect("motion vectors drawn");
    let yellow_px = |x: u32, y: u32| {
        let p = &after[((y * w + x) * 4) as usize..];
        p[0] > 150 && p[1] > 150 && p[2] < 80
    };
    let (mut cab, mut cab_still) = (0, 0);
    for y in 0..h {
        for x in 0..w {
            if yellow_px(x, y) {
                cab += 1;
                let m = mv[(y * w + x) as usize];
                if m[0].abs() * (w as f32) < 0.25 && m[1].abs() * (h as f32) < 0.25 {
                    cab_still += 1;
                }
            }
        }
    }
    assert!(cab > 100, "{cab} dashboard pixels");
    assert_eq!(cab_still, cab, "the dashboard stays where it was on the screen");
    // the road ahead, out of the bus, came towards the camera
    let grey = |x: u32, y: u32| {
        let p = &after[((y * w + x) * 4) as usize..];
        (40..200).contains(&p[0]) && p[0].abs_diff(p[1]) < 12 && p[1].abs_diff(p[2]) < 12
    };
    let road_moved = (0..h).flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| grey(x, y) && mv[(y * w + x) as usize][1].abs() * h as f32 > 0.5)
        .count();
    assert!(road_moved > 50, "{road_moved} road pixels moved");
}


/// The same at a heading the bus box turns with (133 deg, as in the game where the cab came out
/// moving): the eye and the dashboard at the bus's front, 5 m from its origin, the bus driving
/// on. Turned the wrong way, the box left the front outside it and the cab came out moving.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_cab_moves_with_a_turned_vehicle() {
    let (w, h) = (192, 128);
    let mut r = renderer();
    let mut scene = r.new_scene();
    road(&mut r, &mut scene);
    let heading = 133.0f64;
    let forward = DVec3::new(heading.to_radians().sin(), heading.to_radians().cos(), 0.0);
    let (eye, step) = (forward * 5.0, forward * 1.5);
    // (the bus's own frame takes local +y to `forward`: the eye is 5 m ahead of its origin)
    assert!(point_in_vehicle_box(eye + DVec3::new(0.0, 0.0, 2.5), &(DVec3::ZERO, heading, [2.5, 12.0, 3.5, 0.0, 0.0, 1.75])));
    let panel = r.add_mesh(&mut scene, &quad([Vec3::new(-0.4, 1.2, 1.9), Vec3::new(0.4, 1.2, 1.9), Vec3::new(0.4, 1.2, 2.2), Vec3::new(-0.4, 1.2, 2.2)], -Vec3::Y));
    let yellow = r.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 1.0, 0.0, 1.0], true);
    let dash = r.add_instance(&mut scene, panel, eye, Mat4::IDENTITY, vec![yellow]);
    let bus = |o: DVec3| bus_at(o, heading, (0.0, 0.0));
    r.render_to_image(&mut scene, w, h, &camera(eye.x, eye.y, 0.0), &bus(DVec3::ZERO)).unwrap();
    r.set_transform(&mut scene, dash, eye + step, Mat4::IDENTITY);
    let after = r.render_to_image(&mut scene, w, h, &camera(eye.x + step.x, eye.y + step.y, 0.0), &bus(step)).unwrap();
    let (_, _, mv) = r.read_motion().expect("motion vectors drawn");
    let (mut cab, mut still) = (0, 0);
    for y in 0..h {
        for x in 0..w {
            let p = &after[((y * w + x) * 4) as usize..];
            if p[0] > 150 && p[1] > 150 && p[2] < 80 {
                cab += 1;
                let m = mv[(y * w + x) as usize];
                if m[0].abs() * (w as f32) < 0.25 && m[1].abs() * (h as f32) < 0.25 {
                    still += 1;
                }
            }
        }
    }
    assert!(cab > 100, "{cab} dashboard pixels");
    assert_eq!(still, cab, "the dashboard stays where it was on the screen");
}

/// The body pitching back on its springs as the bus pulls away and banking in a turn, the
/// camera held still: a thin bar on the dashboard, followed back along its vectors, lands on
/// the bar where the pitched body had it before (within a pixel) - carried by the heading
/// alone, the cab came out moving at every start and turn - and staying put would miss.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_cab_pitches_and_banks_with_the_body() {
    let (w, h) = (192, 128);
    let mut r = renderer();
    let mut scene = r.new_scene();
    road(&mut r, &mut scene);
    let bar = r.add_mesh(&mut scene, &quad([Vec3::new(-0.5, 1.2, 2.0), Vec3::new(0.5, 1.2, 2.0), Vec3::new(0.5, 1.2, 2.03), Vec3::new(-0.5, 1.2, 2.03)], -Vec3::Y));
    let yellow = r.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 1.0, 0.0, 1.0], true);
    let dash = r.add_instance(&mut scene, bar, DVec3::ZERO, Mat4::IDENTITY, vec![yellow]);
    let before = r.render_to_image(&mut scene, w, h, &camera(0.0, 0.0, 0.0), &bus_at(DVec3::ZERO, 0.0, (0.0, 0.0))).unwrap();
    let tilt = (6f32.to_radians(), 3f32.to_radians());
    let rot = Mat4::from_rotation_x(tilt.0) * Mat4::from_rotation_y(tilt.1);
    r.set_transform(&mut scene, dash, DVec3::ZERO, rot);
    let after = r.render_to_image(&mut scene, w, h, &camera(0.0, 0.0, 0.0), &bus_at(DVec3::ZERO, 0.0, tilt)).unwrap();
    let (_, _, mv) = r.read_motion().expect("motion vectors drawn");
    let yellow_at = |img: &[u8], x: i64, y: i64| x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && {
        let p = &img[((y as u32 * w + x as u32) * 4) as usize..];
        p[0] > 150 && p[1] > 150 && p[2] < 80
    };
    let near_bar = |x: f32, y: f32| (-1..=1).any(|dy| (-1..=1).any(|dx| yellow_at(&before, x.floor() as i64 + dx, y.floor() as i64 + dy)));
    let (mut bar_px, mut back, mut stayed) = (0, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if !yellow_at(&after, x as i64, y as i64) {
                continue;
            }
            bar_px += 1;
            let m = mv[(y * w + x) as usize];
            if near_bar(x as f32 + 0.5 + m[0] * w as f32, y as f32 + 0.5 + m[1] * h as f32) {
                back += 1;
            }
            if near_bar(x as f32 + 0.5, y as f32 + 0.5) {
                stayed += 1;
            }
        }
    }
    assert!(bar_px > 40, "{bar_px} bar pixels");
    assert!(back * 100 > bar_px * 99, "{back} of {bar_px} bar pixels led back onto the bar");
    assert!(stayed * 100 < bar_px * 60, "without the vectors {stayed} of {bar_px} would have: the test tells");
}

/// The motion vectors' shader validates and makes GLSL (OpenGL 3.3, OpenGL ES 3.0): the depth
/// is read as a plain float texture, which GLSL takes (a depth texture's load it does not).
#[test]
fn the_motion_shader_translates_to_glsl() {
    use wgpu::naga;
    use wgpu::naga::back::glsl;
    let src = include_str!("../motion.wgsl");
    let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .expect("validate");
    for version in [glsl::Version::Desktop(330), glsl::Version::Embedded { version: 300, is_webgl: false }] {
        let options = glsl::Options { version, ..Default::default() };
        for entry in &module.entry_points {
            let (module, info) = naga::back::pipeline_constants::process_overrides(&module, &info, Some((entry.stage, &entry.name)), &Default::default())
                .expect("overrides");
            let pipeline = glsl::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone(), multiview: None };
            let mut out = String::new();
            glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, Default::default())
                .and_then(|mut w| w.write())
                .unwrap_or_else(|e| panic!("{version:?} {}: {e:?}", entry.name));
        }
    }
}

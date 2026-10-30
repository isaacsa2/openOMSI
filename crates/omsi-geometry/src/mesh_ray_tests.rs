use super::*;
use crate::ray_mesh;

fn panels(side: usize) -> MeshData {
    let mut mesh = MeshData::default();
    for y in 0..side {
        for x in 0..side {
            let base = mesh.positions.len() as u32;
            let z = ((x * 7 + y * 3) % 11) as f32 * 0.05;
            mesh.positions.extend([
                Vec3::new(x as f32, y as f32, z),
                Vec3::new(x as f32 + 1.0, y as f32, z),
                Vec3::new(x as f32 + 1.0, y as f32 + 1.0, z),
                Vec3::new(x as f32, y as f32 + 1.0, z),
            ]);
            mesh.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    mesh
}

fn compare(mesh: &MeshData, tree: &MeshRaycast, o: Vec3, d: Vec3, xf: &Mat4) {
    let expected = ray_mesh(o, d, mesh, xf);
    let actual = tree.hit(o, d, mesh, xf);
    match (expected, actual) {
        (Some(a), Some(b)) => assert!((a - b).abs() < 1e-4, "{a} != {b}: {o:?}, {d:?}"),
        (None, None) => {}
        _ => panic!("{expected:?} != {actual:?}: {o:?}, {d:?}"),
    }
}

#[test]
fn agrees_with_triangle_scan_for_moved_rotated_scaled_and_mirrored_parts() {
    let mesh = panels(16);
    let tree = MeshRaycast::new(&mesh);
    let transforms = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(20.0, -12.0, 4.0)) * Mat4::from_rotation_x(0.7),
        Mat4::from_rotation_z(-1.1) * Mat4::from_scale(Vec3::new(0.2, 3.0, 1.7)),
        Mat4::from_scale(Vec3::new(-2.0, 0.5, 1.0)),
    ];
    for xf in transforms {
        for x in -2..=18 {
            for y in -2..=18 {
                let o = xf.transform_point3(Vec3::new(x as f32, y as f32, 4.0));
                for local in [-Vec3::Z, Vec3::new(0.2, -0.1, -1.0).normalize(), Vec3::Z] {
                    let d = xf.transform_vector3(local).normalize();
                    compare(&mesh, &tree, o, d, &xf);
                }
            }
        }
    }
}

#[test]
fn parallel_rays_empty_mesh_and_zero_direction_keep_the_old_results() {
    let empty = MeshData::default();
    compare(
        &empty,
        &MeshRaycast::new(&empty),
        Vec3::ZERO,
        Vec3::Z,
        &Mat4::IDENTITY,
    );
    let mesh = panels(8);
    let tree = MeshRaycast::new(&mesh);
    for o in [
        Vec3::ZERO,
        Vec3::new(4.0, 4.0, 0.1),
        Vec3::new(-1.0, 4.0, 0.25),
    ] {
        for d in [Vec3::ZERO, Vec3::X, Vec3::Y, -Vec3::Z] {
            compare(&mesh, &tree, o, d, &Mat4::IDENTITY);
        }
    }
}

#[test]
fn nearest_surface_is_preserved_when_triangles_overlap() {
    let mut mesh = panels(8);
    let base = mesh.positions.len() as u32;
    mesh.positions.extend([
        Vec3::new(0.0, 0.0, 2.0),
        Vec3::new(8.0, 0.0, 2.0),
        Vec3::new(0.0, 8.0, 2.0),
    ]);
    mesh.indices.extend([base, base + 1, base + 2]);
    let o = Vec3::new(1.0, 1.0, 4.0);
    let tree = MeshRaycast::new(&mesh);
    assert_eq!(tree.hit(o, -Vec3::Z, &mesh, &Mat4::IDENTITY), Some(2.0));
    compare(&mesh, &tree, o, -Vec3::Z, &Mat4::IDENTITY);
}

#[test]
fn cone_candidates_match_the_scan_including_wide_and_transformed_cones() {
    let mesh = panels(16);
    let tree = MeshRaycast::new(&mesh);
    let transforms = [
        Mat4::IDENTITY,
        Mat4::from_rotation_x(0.9) * Mat4::from_scale(Vec3::new(-0.2, 3.0, 1.7)),
    ];
    for xf in transforms {
        for x in -2..=18 {
            for y in -2..=18 {
                let o = xf.transform_point3(Vec3::new(x as f32, y as f32, 4.0));
                for d in [-Vec3::Z, Vec3::new(0.2, -0.1, -1.0).normalize(), Vec3::Z] {
                    let d = xf.transform_vector3(d).normalize();
                    for spread in [0.0, 0.001, 0.02, 0.5] {
                        assert_eq!(
                            tree.cone_triangles(o, d, spread, &mesh, &xf),
                            crate::cone_triangles(o, d, spread, &mesh, &xf)
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "synthetic CPU timing; run with --release --ignored --nocapture"]
fn dense_body_picking_timing() {
    let mesh = panels(128);
    let started = std::time::Instant::now();
    let tree = MeshRaycast::new(&mesh);
    let build = started.elapsed();
    let rays: Vec<_> = (0..1000)
        .map(|i| Vec3::new((i % 127) as f32 + 0.25, (i * 31 % 127) as f32 + 0.25, 4.0))
        .collect();
    let scan = std::time::Instant::now();
    let expected: Vec<_> = rays
        .iter()
        .map(|&o| ray_mesh(o, -Vec3::Z, &mesh, &Mat4::IDENTITY))
        .collect();
    let scan = scan.elapsed();
    let bounded = std::time::Instant::now();
    let actual: Vec<_> = rays
        .iter()
        .map(|&o| tree.hit(o, -Vec3::Z, &mesh, &Mat4::IDENTITY))
        .collect();
    let bounded = bounded.elapsed();
    assert_eq!(actual, expected);
    println!("32768 triangles, 1000 rays: build {build:?}, scan {scan:?}, tree {bounded:?}");
}

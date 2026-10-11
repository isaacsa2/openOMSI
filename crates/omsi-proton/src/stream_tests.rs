use crate::{
    convert::Faces,
    tests::{chunk, model},
    three_ds,
};
use glam::{Vec2, Vec3};
use omsi_o3d::{Material, Vertex};
use std::path::PathBuf;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("proton-stream-{}-{n}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self, bytes: &[u8]) -> PathBuf {
        let path = self.0.join("model.3ds");
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn percent(value: f32) -> Vec<u8> {
    chunk(0x0031, &value.to_le_bytes())
}
fn color(value: [f32; 3]) -> Vec<u8> {
    chunk(
        0x0010,
        &value
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

#[test]
fn blender_records_forward_materials_uv_mapping_and_utf8_names_are_recognised() {
    let temp = Temp::new();
    let mut mat = chunk(0xa000, "Árvore\0".as_bytes());
    mat.extend(chunk(0xa020, &color([0.2, 0.4, 0.6])));
    mat.extend(chunk(0xa030, &color([0.8, 0.6, 0.4])));
    mat.extend(chunk(0xa040, &percent(0.5)));
    mat.extend(chunk(0xa041, &percent(0.25)));
    mat.extend(chunk(0xa050, &percent(0.2)));
    mat.extend(chunk(0xa010, &color([0.4, 0.2, 0.1])));
    mat.extend(chunk(0xa084, &percent(0.5)));
    let mut map = chunk(0xa300, "árvore.png\0".as_bytes());
    for (tag, value) in [
        (0xa354, 2.0f32),
        (0xa356, 3.0),
        (0xa358, 0.1),
        (0xa35a, 0.2),
        (0xa35c, std::f32::consts::FRAC_PI_2),
    ] {
        map.extend(chunk(tag, &value.to_le_bytes()));
    }
    mat.extend(chunk(0xa200, &map));
    // Unknown records, cameras and keyframes do not become renderable meshes.
    mat.extend(chunk(0xfefe, &[42; 1024]));
    let road = three_ds::parse(&model()).unwrap().objects.remove(0);
    let mut positions = 3u16.to_le_bytes().to_vec();
    for p in &road.positions {
        for f in p.to_array() {
            positions.extend(f.to_le_bytes());
        }
    }
    let mut faces = [1u16, 0, 1, 2, 0]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut group = "Árvore\0".as_bytes().to_vec();
    group.extend([1u16, 0].into_iter().flat_map(u16::to_le_bytes));
    faces.extend(chunk(0x4130, &group));
    let mut uv = 3u16.to_le_bytes().to_vec();
    for _ in 0..3 {
        uv.extend(0.25f32.to_le_bytes());
        uv.extend(0.5f32.to_le_bytes());
    }
    let mut geo = chunk(0x4110, &positions);
    geo.extend(chunk(0x4120, &faces));
    geo.extend(chunk(0x4140, &uv));
    let mut object = b"road\0".to_vec();
    object.extend(chunk(0x4100, &geo));
    let mut edit = chunk(0x4000, &object);
    edit.extend(chunk(0xafff, &mat)); // material intentionally follows its object
    let mut camera = b"Camera\0".to_vec();
    camera.extend(chunk(0x4700, &[0; 32]));
    edit.extend(chunk(0x4000, &camera));
    let mut root = chunk(0x3d3d, &edit);
    root.extend(chunk(0xb000, &[]));
    let bytes = chunk(0x4d4d, &root);
    let parsed = three_ds::parse(&bytes).unwrap();
    let mut scene = three_ds::Scene::open(&temp.file(&bytes), &|| false).unwrap();
    assert!(scene.keyframes);
    assert_eq!(scene.materials, parsed.materials);
    assert_eq!(scene.materials[0].0, "Árvore");
    let m = &scene.materials[0].1;
    assert_eq!(m.texture, "árvore.png");
    assert_eq!(m.specular, [0.2, 0.15, 0.1]);
    assert_eq!(m.specular_power, 64.0);
    assert_eq!(m.emissive, [0.2, 0.1, 0.05]);
    assert!((m.diffuse[3] - 0.8).abs() < 1e-6);
    let uv = scene.mappings[0].apply(Vec2::new(0.25, 0.5));
    assert!((uv - Vec2::new(-1.4, 0.3)).length() < 1e-6);
    let mut objects = Vec::new();
    scene
        .visit(&|| false, |o| {
            objects.push(o);
            Ok(())
        })
        .unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].positions, parsed.objects[0].positions);
    assert_eq!(objects[0].faces, parsed.objects[0].faces);
    assert_eq!(objects[0].groups, parsed.objects[0].groups);

    std::fs::create_dir_all(temp.0.join("assets/tiles/m")).unwrap();
    std::fs::create_dir_all(temp.0.join("assets/textures")).unwrap();
    std::fs::write(
        temp.0.join("Demo.map.txt"),
        "[map]\nbaseDir=assets\nmodelsDir=m\ntextures=textures\nmapModVersion=4\n",
    )
    .unwrap();
    std::fs::write(temp.0.join("assets/tiles/m/road.3ds"), &bytes).unwrap();
    std::fs::write(
        temp.0.join("assets/tiles/m/entrypoints.txt"),
        "[entrypoint_1]\nname=Start\nposX=299.5\nposY=7\nposZ=10.5\n",
    )
    .unwrap();
    std::fs::write(temp.0.join("assets/textures/árvore.png"), [0]).unwrap();
    let report = crate::import(&temp.0, "maps/Proton-Demo", || false).unwrap();
    assert_eq!(report.tiles, 2);
    assert_eq!(report.warnings.len(), 1); // static import reports the keyframe block
    for file in std::fs::read_dir(temp.0.join("proton-import/model")).unwrap() {
        let mesh = omsi_o3d::load_mesh(&file.unwrap().path()).unwrap();
        let rendered = omsi_geometry::mesh_from_o3d(&mesh);
        assert!(rendered
            .uvs
            .iter()
            .all(|v| (*v - Vec2::new(-1.4, 0.3)).length() < 1e-6));
        assert_eq!(mesh.materials.len(), 1);
        assert_eq!(mesh.materials[0].specular_power, 64.0);
        assert_eq!(mesh.materials[0].texture, "árvore.png");
    }
    assert!(!temp.0.join("proton-import/work").exists());
}

#[test]
fn streaming_rejects_bad_nested_records_and_cancellation_without_accepting_the_tail() {
    let temp = Temp::new();
    let mut bytes = model();
    bytes.extend([0x3d, 0x3d, 200, 0, 0, 0]);
    let mut scene = three_ds::Scene::open(&temp.file(&bytes), &|| false).unwrap();
    assert_eq!(scene.trailing_bytes, 6);
    let mut count = 0;
    scene
        .visit(&|| false, |_| {
            count += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 1);
    assert!(scene.visit(&|| true, |_| Ok(())).is_err());
    for geo in [
        chunk(0x4110, &[3, 0]),
        chunk(0x4110, &[0, 0]),
        chunk(0x4120, &[1, 0, 0, 0, 1, 0, 2, 0, 0, 0]),
        vec![0x10, 0x41, 200, 0, 0, 0],
    ] {
        let mut object = b"bad\0".to_vec();
        object.extend(chunk(0x4100, &geo));
        let bytes = chunk(0x4d4d, &chunk(0x3d3d, &chunk(0x4000, &object)));
        let mut scene = three_ds::Scene::open(&temp.file(&bytes), &|| false).unwrap();
        if geo == chunk(0x4110, &[0, 0]) {
            scene
                .visit(&|| false, |_| panic!("empty mesh must not be emitted"))
                .unwrap();
        } else {
            assert!(scene.visit(&|| false, |_| Ok(())).is_err());
        }
    }
}

#[test]
fn spooled_tiles_survive_writer_eviction_share_vertices_and_clean_up_on_failure() {
    let temp = Temp::new();
    let work = temp.0.join("generated/work");
    let mut spool = crate::spool::Spool::new(&work, &[]).unwrap();
    let face = [Vec3::ZERO, Vec3::X, Vec3::Y].map(|position| Vertex {
        position,
        normal: Vec3::Z,
        uv: Vec2::ZERO,
    });
    for x in 0..40 {
        spool.face((x, 0), face, 0).unwrap();
    }
    spool.face((0, 0), face, 0).unwrap();
    spool.finish_writes().unwrap();
    let (mesh, removed) = spool
        .mesh((0, 0), &[Material::default()], &|| false)
        .unwrap();
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.triangles.len(), 2);
    assert_eq!(mesh.triangles[0].indices, mesh.triangles[1].indices);
    assert_eq!(removed, 3);
    assert!(spool
        .mesh((0, 0), &[Material::default()], &|| true)
        .is_err());
    std::fs::write(work.join("0_0.faces"), [0]).unwrap();
    assert!(spool
        .mesh((0, 0), &[Material::default()], &|| false)
        .is_err());
    drop(spool);
    assert!(!work.exists());
}

#[test]
fn identical_materials_across_files_share_source_slots_but_different_uv_maps_do_not() {
    let temp = Temp::new();
    let mut edit = chunk(0xafff, &chunk(0xa000, b"A\0"));
    edit.extend(chunk(0xafff, &chunk(0xa000, b"B\0")));
    let bytes = chunk(0x4d4d, &chunk(0x3d3d, &edit));
    let mut scene = three_ds::Scene::open(&temp.file(&bytes), &|| false).unwrap();
    let mut registry = crate::materials::Registry::default();
    let (names, _) = registry.register(&scene).unwrap();
    assert_eq!(names["A"], names["B"]);
    let count = registry.materials.len();
    registry.register(&scene).unwrap();
    assert_eq!(registry.materials.len(), count);
    scene.mappings[1].scale.x = 2.0;
    let (names, _) = registry.register(&scene).unwrap();
    assert_ne!(names["A"], names["B"]);
}

#[test]
fn cancelling_a_map_import_removes_the_spool_without_publishing_a_manifest() {
    let temp = Temp::new();
    std::fs::create_dir_all(temp.0.join("assets/tiles/m")).unwrap();
    std::fs::write(
        temp.0.join("Demo.map.txt"),
        "[map]\nbaseDir=assets\nmodelsDir=m\nmapModVersion=4\n",
    )
    .unwrap();
    std::fs::write(temp.0.join("assets/tiles/m/road.3ds"), model()).unwrap();
    std::fs::write(
        temp.0.join("assets/tiles/m/entrypoints.txt"),
        "[entrypoint_1]\nname=Start\nposX=299.5\nposY=7\nposZ=10.5\n",
    )
    .unwrap();
    let calls = std::cell::Cell::new(0);
    let error = crate::import(&temp.0, "maps/Proton-Demo", || {
        let n = calls.get() + 1;
        calls.set(n);
        n >= 6
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("cancelled"));
    assert!(!temp.0.join("proton-import/work").exists());
    assert!(!temp.0.join("global.cfg").exists());
}

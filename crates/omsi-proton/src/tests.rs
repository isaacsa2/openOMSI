use super::*;
use crate::convert::Tiles;
use glam::{Vec2, Vec3};
use omsi_o3d::{Mesh, Triangle, Vertex};

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("omsi-proton-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn chunk(tag: u16, data: &[u8]) -> Vec<u8> {
    let mut out = tag.to_le_bytes().to_vec();
    out.extend_from_slice(&((data.len() + 6) as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

pub(super) fn model() -> Vec<u8> {
    named_model("road")
}

pub(super) fn named_model(name: &str) -> Vec<u8> {
    let mut positions = 3u16.to_le_bytes().to_vec();
    for p in [
        [299.0f32, 10.0, 7.0],
        [301.0, 10.0, 7.0],
        [299.0, 12.0, 7.0],
    ] {
        for f in p {
            positions.extend_from_slice(&f.to_le_bytes());
        }
    }
    let faces = [1u16, 0, 1, 2, 0]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let mut geo = chunk(0x4110, &positions);
    geo.extend(chunk(0x4120, &faces));
    // A nonzero local-axis origin must not translate world-space vertices a second time.
    let matrix = [
        1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 100.0, 20.0, 7.0,
    ];
    geo.extend(chunk(
        0x4160,
        &matrix
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    ));
    let mut object = name.as_bytes().to_vec();
    object.push(0);
    object.extend(chunk(0x4100, &geo));
    chunk(0x4d4d, &chunk(0x3d3d, &chunk(0x4000, &object)))
}

pub(super) fn setup(f: &Fixture) {
    std::fs::create_dir_all(f.0.join("Town/tiles/0x0")).unwrap();
    std::fs::create_dir_all(f.0.join("Town/textures")).unwrap();
    std::fs::write(
        f.0.join("Town.map.txt"),
        "[map]\nbaseDir=Town\nmodelsDir=0x0\ntextures=textures\nmapModVersion=4\n",
    )
    .unwrap();
    std::fs::write(f.0.join("Town/tiles/0x0/road.3ds"), model()).unwrap();
    std::fs::write(
        f.0.join("Town/tiles/0x0/entrypoints.txt"),
        "[entrypoint_1]\nname=Start\nposX=299.5\nposY=7\nposZ=10.5\nrotY=90\n",
    )
    .unwrap();
}

#[test]
fn import_roundtrip_keeps_tile_boundaries_height_spawn_and_surface() {
    let f = Fixture::new();
    setup(&f);
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.tiles, 2);
    let global = omsi_map::GlobalCfg::load(&f.0.join("global.cfg")).unwrap();
    assert_eq!(global.tiles.len(), 2);
    assert_eq!(global.entry_points[0].pos, [299.5, 10.5, 7.0]);
    let ep = &global.entry_points[0];
    assert!((2.0 * ep.quat[1].atan2(ep.quat[3]).to_degrees() - 90.0).abs() < 1e-6);
    let mut area = 0.0;
    for t in global.tiles {
        let tile = omsi_map::Tile::load(&f.0.join(&t.file)).unwrap();
        let terrain = omsi_map::Terrain::load(&f.0.join(format!("{}.terrain", t.file))).unwrap();
        assert!(terrain.heights.iter().all(|&h| h < 7.0));
        assert_eq!(tile.objects.len(), 1);
        assert_eq!(tile.objects[0].pos, [0.0; 3]);
        let file = Path::new(&tile.objects[0].file).file_name().unwrap();
        let sco = omsi_scenery::SceneryObject::load(&f.0.join("proton-import").join(file)).unwrap();
        assert!(sco.surface && sco.abs_height);
        let mesh = omsi_o3d::load_mesh(
            &f.0.join("proton-import/model")
                .join(format!("tile_{}_{}.o3d", t.x, t.y)),
        )
        .unwrap();
        // Exercise the game's geometry loader, not just the raw file parser. O3D stores
        // Y up and this loader swaps it back to Z up before rendering and wheel probes.
        let rendered = omsi_geometry::mesh_from_o3d(&mesh);
        for (p, n) in rendered.positions.iter().zip(&rendered.normals) {
            assert!((0.0..=300.0).contains(&p.x));
            assert!((10.0..=12.0).contains(&p.y));
            assert_eq!(p.z, 7.0);
            assert_eq!(*n, Vec3::Z);
        }
        for face in rendered.indices.chunks_exact(3) {
            let [a, b, c] = [face[0], face[1], face[2]].map(|i| rendered.positions[i as usize]);
            assert!(
                (b - a).cross(c - a).z < 0.0,
                "road must face up with clockwise world winding"
            );
            area += (b - a).cross(c - a).length() * 0.5;
        }
    }
    assert!(
        (area - 2.0).abs() < 1e-6,
        "clipping must neither lose nor duplicate road area"
    );
}

#[test]
fn inline_base_directory_works() {
    let f = Fixture::new();
    setup(&f);
    std::fs::rename(f.0.join("Town/tiles"), f.0.join("tiles")).unwrap();
    std::fs::rename(f.0.join("Town/textures"), f.0.join("textures")).unwrap();
    std::fs::write(
        f.0.join("Town.map.txt"),
        "[map]\nbaseDir=.\nmodelsDir=./0x0/\ntextures=./textures/\nmapModVersion=4\n",
    )
    .unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.tiles, 2);
    assert!(f.0.join("proton-import/model/tile_0_0.o3d").is_file());
}

#[test]
fn gps_files_and_embedded_helpers_are_omitted_and_relative_paths_work() {
    let f = Fixture::new();
    setup(&f);
    let road = named_model("road");
    let gps = named_model("_162_gps_");
    let mut objects = road[12..].to_vec();
    objects.extend_from_slice(&gps[12..]);
    std::fs::write(
        f.0.join("Town/tiles/0x0/road.3ds"),
        chunk(0x4d4d, &chunk(0x3d3d, &objects)),
    )
    .unwrap();
    std::fs::write(f.0.join("Town/tiles/0x0/GPS.3ds"), model()).unwrap();
    std::fs::write(
        f.0.join("Town.map.txt"),
        "[map]\nbaseDir=./Town/\nmodelsDir=./0x0/\ntextures=./textures/\nmapModVersion=3\n",
    )
    .unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.gps_skipped, 2);
    assert_eq!(report.tiles, 2);
    assert!(report.vertices_removed > 0);
    assert!(!optimise::gps_name("gpsmall"));
    assert!(
        f.0.join("Town/tiles/0x0/GPS.3ds").exists(),
        "source assets stay intact"
    );
    assert!(child(&f.0, "./Town/../outside").is_err());
}

#[test]
fn tile_compaction_keeps_uv_seams_normals_and_material_properties() {
    let mut tiles = Tiles::new();
    let v = |x, y| Vertex {
        position: Vec3::new(x, y, 7.0),
        normal: Vec3::Z,
        uv: Vec2::new(x, y),
    };
    let mut seam = v(1.0, 0.0);
    seam.uv = Vec2::new(0.25, 0.75);
    let mut sharp = v(0.0, 0.0);
    sharp.normal = -Vec3::Z;
    tiles.insert(
        (0, 0),
        Mesh {
            vertices: vec![
                v(0.0, 0.0),
                v(1.0, 0.0),
                v(0.0, 1.0),
                v(0.0, 0.0),
                seam,
                v(0.0, 1.0),
                sharp,
            ],
            triangles: vec![
                Triangle {
                    indices: [0, 1, 2],
                    material: 0,
                },
                Triangle {
                    indices: [3, 4, 5],
                    material: 1,
                },
                Triangle {
                    indices: [6, 2, 1],
                    material: 0,
                },
            ],
            ..Default::default()
        },
    );
    let m = Material {
        texture: "leaf.png".into(),
        diffuse: [0.5, 0.6, 0.7, 0.4],
        ..Default::default()
    };
    let mut materials = vec![
        m.clone(),
        m.clone(),
        Material {
            texture: "unused.png".into(),
            ..Default::default()
        },
    ];
    let mut builder = optimise::Builder::default();
    for t in &tiles[&(0, 0)].triangles {
        builder.face(
            t.indices.map(|i| tiles[&(0, 0)].vertices[i as usize]),
            t.material,
            &materials[t.material as usize],
        );
    }
    optimise::used_materials(&mut tiles, &mut materials);
    assert_eq!(materials.len(), 2);
    assert_eq!(
        optimise::prepare(&mut tiles, &materials, &|| false).unwrap(),
        2
    );
    let mesh = &tiles[&(0, 0)];
    assert_eq!(builder.mesh.vertices, mesh.vertices);
    assert_eq!(builder.mesh.triangles, mesh.triangles);
    assert_eq!(builder.mesh.materials, mesh.materials);
    assert_eq!(mesh.vertices.len(), 5);
    assert_eq!(mesh.materials, [m]);
    assert!(mesh.triangles.iter().all(|t| t.material == 0));
    assert_ne!(mesh.triangles[0].indices[1], mesh.triangles[1].indices[1]);
    assert_eq!(mesh.vertices[mesh.triangles[1].indices[1] as usize], seam);
    assert_eq!(mesh.vertices[mesh.triangles[2].indices[0] as usize], sharp);
}

#[test]
fn trailing_data_after_the_root_is_reported_but_bad_nested_chunks_are_rejected() {
    let f = Fixture::new();
    setup(&f);
    let mut bytes = model();
    // Not a valid chunk: this must never be read as part of the declared model.
    bytes.extend_from_slice(&[1, 77, 62, 0, 0, 104, 63, 181]);
    let parsed = three_ds::parse(&bytes).unwrap();
    assert_eq!(parsed.trailing_bytes, 8);
    assert_eq!(
        parsed.objects[0].positions,
        three_ds::parse(&model()).unwrap().objects[0].positions
    );
    std::fs::write(f.0.join("Town/tiles/0x0/road.3ds"), bytes).unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.tiles, 2);
    assert_eq!(
        report.warnings,
        ["road.3ds: ignored 8 trailing bytes outside the declared 3DS root chunk"]
    );
    let nested = chunk(0x4d4d, &[0x3d, 0x3d, 200, 0, 0, 0]);
    assert!(three_ds::parse(&nested).is_err());
}

#[test]
fn rejects_damaged_meshes_and_unsafe_or_unsupported_map_configs() {
    for bad in [
        vec![],
        vec![0x4d, 0x4d, 0, 0, 0, 0],
        vec![0x4d, 0x4d, 100, 0, 0, 0],
    ] {
        assert!(three_ds::parse(&bad).is_err());
    }
    let f = Fixture::new();
    setup(&f);
    assert!(child(&f.0, "../outside").is_err());
    assert!(child(&f.0, "C:\\outside").is_err());
    assert!(child(&f.0, "/outside").is_err());
    assert!(import(&f.0, "maps/Proton-Town", || true).is_err());
    assert!(!f.0.join("global.cfg").exists());
    std::fs::write(
        f.0.join("Town.map.txt"),
        "[map]\nbaseDir=../outside\nmapModVersion=4",
    )
    .unwrap();
    assert!(import(&f.0, "maps/Proton-Town", || false).is_err());
}

#[test]
fn o3d_writer_preserves_indices_material_opacity_and_uv() {
    let f = Fixture::new();
    let v = Vertex {
        position: Vec3::new(2.0, 3.0, 4.0),
        normal: Vec3::Z,
        uv: Vec2::new(0.25, 0.75),
    };
    let mesh = Mesh {
        vertices: vec![v; 3],
        triangles: vec![Triangle {
            indices: [0, 1, 2],
            material: 0,
        }],
        ..Default::default()
    };
    let material = Material {
        texture: "árvore.PNG".into(),
        diffuse: [1.0, 0.5, 0.2, 0.4],
        ..Default::default()
    };
    convert::write_mesh(&f.0.join("test.o3d"), &mesh, &[material.clone()]).unwrap();
    let loaded = omsi_o3d::load_mesh(&f.0.join("test.o3d")).unwrap();
    assert!(loaded
        .vertices
        .iter()
        .all(|v| v.position == Vec3::new(2.0, 4.0, 3.0) && v.normal == Vec3::Y));
    assert_eq!(loaded.triangles[0].indices, [0, 2, 1]);
    let rendered = omsi_geometry::mesh_from_o3d(&loaded);
    assert_eq!(rendered.positions, vec![v.position; 3]);
    assert_eq!(rendered.normals, vec![v.normal; 3]);
    assert_eq!(rendered.uvs, vec![v.uv; 3]);
    assert_eq!(loaded.materials, vec![material]);
}

#[test]
fn stale_entry_point_is_relocated_and_reported_without_inventing_a_route() {
    let f = Fixture::new();
    setup(&f);
    std::fs::write(
        f.0.join("Town/tiles/0x0/entrypoints.txt"),
        "[entrypoint_1]\nname=Old route\nposX=0\nposY=0\nposZ=0\n",
    )
    .unwrap();
    // This test's tiny synthetic face is intentionally below the fallback area threshold.
    let mut tiles = convert::Tiles::new();
    let mut m = three_ds::parse(&model()).unwrap();
    for p in &mut m.objects[0].positions {
        p.y *= 10.0;
    }
    convert::append(m, &mut tiles, &mut Vec::new(), &[], &|| false).unwrap();
    let mut spawns = spawns(&f.0.join("Town"), &f.0.join("Town/tiles/0x0")).unwrap();
    let mut warnings = Vec::new();
    spawn::validate(&tiles, &mut spawns, &mut warnings).unwrap();
    assert_eq!(spawns[0].name, "Old route");
    assert!(spawns[0].pos[0] >= 299.0 && spawns[0].pos[0] <= 301.0);
    assert_eq!(spawns[0].pos[2], 7.5);
    assert_eq!(warnings.len(), 1);
}

/// Real mods stay outside the repository. This checks the local conversion with the same
/// format loaders used by the game; it does not substitute for a driving/GPU test.
#[test]
fn supplied_map_fixture_when_available() {
    let Some(dir) = std::env::var_os("PROTON_MAP_FIXTURE") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let name = std::env::var("PROTON_MAP_NAME").unwrap_or_else(|_| "Fixture".into());
    let report = import(&dir, &format!("maps/Proton-{name}"), || false).unwrap();
    let global = omsi_map::GlobalCfg::load(&dir.join("global.cfg")).unwrap();
    assert!(!global.entry_points.is_empty());
    let mut vertices = 0;
    let mut materials = 0;
    for t in &global.tiles {
        let tile = omsi_map::Tile::load(&dir.join(&t.file)).unwrap();
        assert!(!tile.objects.is_empty());
        for object in tile.objects {
            let file = Path::new(&object.file).file_name().unwrap();
            let sco =
                omsi_scenery::SceneryObject::load(&dir.join("proton-import").join(file)).unwrap();
            let models = sco
                .model
                .meshes
                .iter()
                .map(|m| m.file.as_str())
                .chain(sco.collision_mesh.as_deref());
            for model in models {
                let mesh =
                    omsi_o3d::load_mesh(&dir.join("proton-import/model").join(model)).unwrap();
                assert!(!mesh.triangles.is_empty());
                vertices += mesh.vertices.len();
                materials += mesh.materials.len();
                assert!(mesh
                    .vertices
                    .iter()
                    .all(|v| v.position.is_finite() && v.normal.is_finite()));
                assert!(mesh
                    .triangles
                    .iter()
                    .all(|t| (t.material as usize) < mesh.materials.len()));
            }
        }
    }
    eprintln!(
        "Imported {} tiles, {} triangles, {} skipped markers, {} GPS helpers, {} duplicate vertices removed; {vertices} vertices and {materials} material slots exported; warnings: {:?}",
        report.tiles, report.triangles, report.markers_skipped, report.gps_skipped, report.vertices_removed, report.warnings
    );
}

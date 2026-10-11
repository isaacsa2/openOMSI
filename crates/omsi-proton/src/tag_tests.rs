use crate::{
    import,
    tags::{classify, Mode, ObjectKind},
    tests::{chunk, model, named_model, setup, Fixture},
};
use glam::{DVec3, Mat4};

fn combined(names: &[&str], texture: &str) -> Vec<u8> {
    let mut material = chunk(0xa000, b"shared\0");
    let mut filename = texture.as_bytes().to_vec();
    filename.push(0);
    material.extend(chunk(0xa200, &chunk(0xa300, &filename)));
    let mut edit = chunk(0xafff, &material);
    let object = crate::three_ds::parse(&model()).unwrap().objects.remove(0);
    for name in names {
        let mut positions = 3u16.to_le_bytes().to_vec();
        for p in &object.positions {
            positions.extend(p.to_array().into_iter().flat_map(f32::to_le_bytes));
        }
        let mut faces: Vec<_> = [1u16, 0, 1, 2, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut group = b"shared\0".to_vec();
        group.extend([1u16, 0].into_iter().flat_map(u16::to_le_bytes));
        faces.extend(chunk(0x4130, &group));
        let mut mesh = chunk(0x4110, &positions);
        mesh.extend(chunk(0x4120, &faces));
        let mut object = name.as_bytes().to_vec();
        object.push(0);
        object.extend(chunk(0x4100, &mesh));
        edit.extend(chunk(0x4000, &object));
    }
    chunk(0x4d4d, &chunk(0x3d3d, &edit))
}

#[test]
fn shared_texture_keeps_blend_cutout_and_untagged_material_slots_distinct() {
    for texture in ["glass.PNG", "glass.bmp"] {
        let f = Fixture::new();
        setup(&f);
        let bytes = combined(
            &["glass_transparent_.001", "fence_cutout_", "road"],
            texture,
        );
        std::fs::write(f.0.join("Town/tiles/0x0/road.3ds"), bytes).unwrap();
        std::fs::write(f.0.join("Town/textures").join(texture), [0]).unwrap();
        let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
        assert!(report.warnings.is_empty());
        let global = omsi_map::GlobalCfg::load(&f.0.join("global.cfg")).unwrap();
        for tile in global.tiles {
            let stem = format!("tile_{}_{}", tile.x, tile.y);
            let sco =
                omsi_scenery::SceneryObject::load(&f.0.join(format!("proton-import/{stem}.sco")))
                    .unwrap();
            let mesh =
                omsi_o3d::load_mesh(&f.0.join(format!("proton-import/model/{stem}.o3d"))).unwrap();
            assert_eq!(
                mesh.materials.len(),
                3,
                "shader modes must survive compaction"
            );
            assert!(mesh
                .materials
                .iter()
                .all(|m| m.texture == texture && m.diffuse[3] == 1.0));
            assert!((0..3).all(|slot| mesh.triangles.iter().any(|t| t.material == slot)));
            let overrides = &sco.model.meshes[0].materials;
            let expected = if texture.ends_with("PNG") {
                vec![(0, 2), (1, 1), (2, 1)]
            } else {
                vec![(0, 2), (1, 1)]
            };
            assert_eq!(
                overrides
                    .iter()
                    .map(|m| (m.index, m.alpha))
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(overrides
                .iter()
                .all(|m| m.texture == texture && m.alpha_set));
        }
        assert_eq!(
            std::fs::read_dir(f.0.join("proton-import/texture"))
                .unwrap()
                .count(),
            1
        );
    }
}

#[test]
fn invisible_collider_is_not_rendered_and_retains_clipped_wheel_support() {
    for with_visible in [false, true] {
        let f = Fixture::new();
        setup(&f);
        let names = if with_visible {
            vec!["road", "floor_gencol_invisible_"]
        } else {
            vec!["floor_gencol_invisible_"]
        };
        std::fs::write(
            f.0.join("Town/tiles/0x0/road.3ds"),
            combined(&names, "unavailable.bmp"),
        )
        .unwrap();
        let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
        assert_eq!(report.warnings.len(), usize::from(with_visible));
        assert_eq!(
            std::fs::read_dir(f.0.join("proton-import/texture"))
                .unwrap()
                .count(),
            0
        );
        let global = omsi_map::GlobalCfg::load(&f.0.join("global.cfg")).unwrap();
        assert_eq!(global.entry_points[0].pos, [299.5, 10.5, 7.0]);
        let mut area = 0.0;
        let mut ids = std::collections::BTreeSet::new();
        for t in global.tiles {
            let tile = omsi_map::Tile::load(&f.0.join(&t.file)).unwrap();
            assert_eq!(tile.objects.len(), 1 + usize::from(with_visible));
            for object in &tile.objects {
                assert!(ids.insert(object.id));
            }
            let collision = format!("tile_{}_{}_collision", t.x, t.y);
            let sco = omsi_scenery::SceneryObject::load(
                &f.0.join(format!("proton-import/{collision}.sco")),
            )
            .unwrap();
            assert!(
                sco.model.meshes.is_empty(),
                "no render/shadow mesh may be exported"
            );
            assert!(sco.fixed && sco.abs_height && !sco.no_collision);
            assert_eq!(
                sco.collision_mesh.as_deref(),
                Some(format!("{collision}.o3d").as_str())
            );
            let mesh =
                omsi_o3d::load_mesh(&f.0.join(format!("proton-import/model/{collision}.o3d")))
                    .unwrap();
            assert!(mesh.materials.iter().all(|m| m.texture.is_empty()));
            let geometry = omsi_geometry::mesh_from_o3d(&mesh);
            for tri in geometry.indices.chunks_exact(3) {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| geometry.positions[i as usize]);
                area += (b - a).cross(c - a).length() * 0.5;
            }
            let mut surface = omsi_geometry::TileSurface::new(64);
            surface.add_drive_mesh(&geometry, &Mat4::IDENTITY, DVec3::ZERO, 0, 0);
            surface.finish();
            let x = if t.x == 0 { 299.5 } else { 0.1 };
            assert!((surface.drive.probe(x, 10.1, 8.0).below.unwrap() - 7.0).abs() < 1e-5);
            let visible = omsi_o3d::load_mesh(
                &f.0.join(format!("proton-import/model/tile_{}_{}.o3d", t.x, t.y)),
            )
            .unwrap();
            assert_eq!(
                visible.triangles.len(),
                if with_visible {
                    mesh.triangles.len()
                } else {
                    0
                }
            );
        }
        assert!((area - 2.0).abs() < 1e-6);
    }
}

#[test]
fn gameplay_volumes_and_embedded_tree_boxes_are_omitted_and_reported() {
    let f = Fixture::new();
    setup(&f);
    let mut edit = named_model("road")[12..].to_vec();
    for name in [
        "zone_low_speed_zone_",
        "zone_force_exit_gencol_",
        "zone_spawn_blocker_radius_",
        "helper_invisible_",
        "tree_utree101_a",
    ] {
        edit.extend_from_slice(&named_model(name)[12..]);
    }
    std::fs::write(
        f.0.join("Town/tiles/0x0/road.3ds"),
        chunk(0x4d4d, &chunk(0x3d3d, &edit)),
    )
    .unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.warnings.len(), 2);
    assert!(report.warnings.iter().any(|w| w.starts_with("4 invisible")));
    assert!(report
        .warnings
        .iter()
        .any(|w| w.starts_with("1 embedded tree")));
    // The tiny road clips into three faces; all five helper geometries must be absent.
    assert_eq!(report.triangles, 3);
    for name in [
        "not_transparentish_",
        "not_cutoutish_",
        "invisible_house",
        "tree_utree10_a",
        "tree_utree1010_a",
        "GLASS_TRANSPARENT_",
    ] {
        assert!(
            matches!(classify(name), ObjectKind::Mesh(Mode::Auto)),
            "{name}"
        );
    }
}

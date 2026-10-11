use crate::{
    import,
    tests::{chunk, named_model, setup, Fixture},
};
use glam::Vec3;

fn config(f: &Fixture, folder: &str, name: &str, contents: &str) {
    let path = f.0.join("Town/tiles/0x0").join(folder);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join(name), contents).unwrap();
}

fn objects(f: &Fixture, objects: &[(&str, [f32; 3])]) {
    let mut edit = named_model("road")[12..].to_vec();
    for (name, p) in objects {
        let p = Vec3::from(*p);
        let mut positions = 3u16.to_le_bytes().to_vec();
        for v in [
            p + Vec3::new(-0.5, -0.5, 0.0),
            p + Vec3::new(0.5, -0.5, 0.0),
            p + Vec3::new(-0.5, 0.5, 0.0),
        ] {
            positions.extend(v.to_array().into_iter().flat_map(f32::to_le_bytes));
        }
        let faces = [1u16, 0, 1, 2, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut mesh = chunk(0x4110, &positions);
        mesh.extend(chunk(0x4120, &faces));
        let mut object = name.as_bytes().to_vec();
        object.push(0);
        object.extend(chunk(0x4100, &mesh));
        edit.extend(chunk(0x4000, &object));
    }
    std::fs::write(
        f.0.join("Town/tiles/0x0/road.3ds"),
        chunk(0x4d4d, &chunk(0x3d3d, &edit)),
    )
    .unwrap();
}

fn loaded(f: &Fixture) -> Vec<omsi_map::Tile> {
    omsi_map::GlobalCfg::load(&f.0.join("global.cfg"))
        .unwrap()
        .tiles
        .iter()
        .map(|t| omsi_map::Tile::load(&f.0.join(&t.file)).unwrap())
        .collect()
}

fn sco(f: &Fixture, object: &omsi_map::MapObject) -> omsi_scenery::SceneryObject {
    let rel = object.file.split("/proton-import/").nth(1).unwrap();
    omsi_scenery::SceneryObject::load(&f.0.join("proton-import").join(rel)).unwrap()
}

#[test]
fn vertex_only_3ds_markers_create_native_paths_without_rendering_helper_geometry() {
    let f = Fixture::new();
    setup(&f);
    let mut edit = named_model("road")[12..].to_vec();
    for (name, pos) in [
        ("cars.000", [299.0f32, 10.0, 7.0]),
        ("cars.001", [301.0, 10.0, 7.0]),
        ("walk.000", [299.0, 8.0, 7.0]),
        ("walk.001", [301.0, 8.0, 7.0]),
    ] {
        let mut positions = 1u16.to_le_bytes().to_vec();
        positions.extend(pos.into_iter().flat_map(f32::to_le_bytes));
        let mut mesh = chunk(0x4110, &positions);
        mesh.extend(chunk(0x4120, &0u16.to_le_bytes()));
        let mut object = name.as_bytes().to_vec();
        object.push(0);
        object.extend(chunk(0x4100, &mesh));
        edit.extend(chunk(0x4000, &object));
    }
    let bytes = chunk(0x4d4d, &chunk(0x3d3d, &edit));
    let parsed = crate::three_ds::parse(&bytes).unwrap();
    assert_eq!(parsed.objects.len(), 5);
    assert!(parsed.objects[1..]
        .iter()
        .all(|o| o.positions.len() == 1 && o.faces.is_empty()));
    std::fs::write(f.0.join("Town/tiles/0x0/road.3ds"), bytes).unwrap();
    config(
        &f,
        "aivehicles",
        "cars.txt",
        "[from_3d]\nreadFrom3D=1\nprefix=cars\n",
    );
    config(
        &f,
        "aipeople",
        "walk.txt",
        "[from_3d]\nreadFrom3D=1\nprefix=walk\n",
    );
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.roads_imported, 1);
    assert_eq!(report.traffic_segments_imported, 2);
    assert_eq!(report.pedestrian_segments_imported, 2);
    assert_eq!(report.markers_skipped, 4);
    assert_eq!(report.triangles, 3);
    let paths: usize = loaded(&f)
        .iter()
        .flat_map(|t| &t.objects)
        .map(|o| sco(&f, o).paths.len())
        .sum();
    assert_eq!(paths, 4);
}

#[test]
fn native_paths_stops_waiting_and_lamps_survive_an_end_to_end_import() {
    let f = Fixture::new();
    setup(&f);
    objects(
        &f,
        &[
            ("stop_trigger", [301.0, 10.0, 7.0]),
            ("stop.000", [301.0, 8.0, 7.0]),
            ("stop.001", [302.0, 8.0, 7.0]),
            ("_junction_path1_trigger_", [305.0, 10.0, 8.0]),
            ("_junction_path1_red_emissive_", [305.0, 9.0, 9.0]),
            ("_junction_path1_green_additive_", [305.0, 9.0, 8.0]),
        ],
    );
    config(&f,"aivehicles","road.txt","[000]\nposX=292\nposZ=10\nposY=7\n[001]\nposX=315\nposZ=10\nposY=7\n[defaults]\nspawnInterval=10\napplySpeedMultiplier=1\nspeedMultiplier=0.8\nrightBlinker=1\nisBusSpawner=1\n");
    config(&f,"aipeople","walk.txt","[000]\nposX=299\nposZ=8\nposY=7\n[001]\nposX=315\nposZ=8\nposY=7\n[automatic_setup]\nreverse=1\n");
    config(&f,"busstops","stop.txt","[busstop]\nname=Parada São João\npaxAmount=25\n[from_3d]\nreadFrom3D=1\nprefix=stop\nmaxPathsToCheck=2\n");
    config(&f,"trafficlights","signal.txt","[trafficlight]\nprefix=junction\nhowManyPaths=1\nhowManyTicks=2\ntickInterval=2\nrandomTimestampAtStart=0\nuseRealLights=0\n[tick1]\nrepeat=2\npath1_red=1\npath1_trigger=1\n[tick2]\nrepeat=3\npath1_green=1\n");
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.stops_imported, 1);
    assert_eq!(report.signals_imported, 1);
    assert_eq!(report.signal_paths_imported, 1);
    assert_eq!(report.pedestrian_segments_imported, 2);
    assert_eq!(report.traffic_segments_imported, 3);
    assert_eq!(
        report.triangles, 3,
        "simulation and lamp geometry stay out of the static road mesh"
    );
    let tiles = loaded(&f);
    let all: Vec<_> = tiles.iter().flat_map(|t| &t.objects).collect();
    let stop = all.iter().find(|o| sco(&f, o).is_bus_stop).unwrap();
    assert_eq!(
        stop.extra,
        vec!["Parada São João", "2", "2", "10", "30", "0"]
    );
    assert_eq!(stop.rot, [90.0, 0.0, 0.0]);
    assert_eq!(stop.pos, [1.0, 10.0, 7.0]);
    assert_eq!(
        all.iter()
            .filter(|o| sco(&f, o).passenger_cabin.is_some())
            .count(),
        2
    );
    let controller = all
        .iter()
        .find(|o| !sco(&f, o).traffic_lights.is_empty())
        .unwrap();
    let program = sco(&f, controller);
    assert_eq!(program.traffic_lights_group, Some(10.0));
    assert_eq!(program.traffic_lights.len(), 4);
    assert_eq!(
        program.traffic_lights[3]
            .phases
            .iter()
            .map(|p| (p.state, p.duration))
            .collect::<Vec<_>>(),
        vec![(0, 4.0), (12, 6.0)]
    );
    assert_eq!(
        program.paths[0].start,
        [5.0, 10.0, 7.0],
        "stop line splits the lane at the trigger, not at its owner tile's edge"
    );
    assert_eq!(program.path_traffic_light, [3]);
    let lamps: Vec<_> = all.iter().filter(|o| o.file.contains("/signal_")).collect();
    assert_eq!(lamps.len(), 2);
    for lamp in lamps {
        assert_eq!(lamp.var_parent, Some(controller.id));
        let sco = sco(&f, lamp);
        assert!(sco.model.meshes[0].visible.is_some());
        assert!(sco.model.meshes[0].materials[0].allcolor.is_some());
    }
    let plain = all
        .iter()
        .find(|o| {
            !sco(&f, o).paths.is_empty()
                && sco(&f, o).traffic_lights.is_empty()
                && o.file.contains("paths_1_0")
        })
        .unwrap();
    assert!(plain
        .rules
        .iter()
        .any(|r| r.kind == "speedlimit" && r.value == 40.0));
    assert!(plain
        .rules
        .iter()
        .any(|r| r.kind == "trafficdensity" && r.value == 0.5));
    assert!(plain.rules.iter().any(|r| r.kind == "bus"));
}

#[test]
fn decorative_signals_never_hold_traffic_without_authored_triggers() {
    let f = Fixture::new();
    setup(&f);
    objects(&f, &[("flash_path1_yellow_a", [301.0, 10.0, 8.0])]);
    config(&f,"trafficlights","flash.txt","[trafficlight]\nprefix=flash\nhowManyPaths=1\nhowManyTicks=2\ntickInterval=0.8\nrandomTimestampAtStart=0\nuseRealLights=0\n[tick1]\npath1_yellow=1\n[tick2]\n");
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.signals_imported, 1);
    assert_eq!(report.signal_paths_imported, 0);
    let tiles = loaded(&f);
    let program = tiles
        .iter()
        .flat_map(|t| &t.objects)
        .map(|o| sco(&f, o))
        .find(|s| !s.traffic_lights.is_empty())
        .unwrap();
    assert!(program.traffic_lights[3]
        .phases
        .iter()
        .all(|p| p.state == 12));
    assert_eq!(
        program.traffic_lights[1]
            .phases
            .iter()
            .map(|p| p.state)
            .collect::<Vec<_>>(),
        vec![9, 12]
    );
}

#[test]
fn coordinate_stops_preserve_left_platform_and_rotated_waiting_positions() {
    let f = Fixture::new();
    setup(&f);
    config(&f,"busstops","left.txt","[busstop]\nname=[TH3D] Platform\nposX=301\nposZ=10\nposY=7\nrotY=90\nisLeft=1\npaxAmount=1\n[standing_1]\nposX=-3\nposZ=0\nposY=0\nrotY=30\n");
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.stops_imported, 1);
    let tiles = loaded(&f);
    let all: Vec<_> = tiles.iter().flat_map(|t| &t.objects).collect();
    let stop = all.iter().find(|o| sco(&f, o).is_bus_stop).unwrap();
    assert_eq!(stop.extra[0], "[TH3D] Platform");
    assert_eq!(stop.extra[5], "1");
    let waiting = all
        .iter()
        .find(|o| sco(&f, o).passenger_cabin.is_some())
        .unwrap();
    assert!((waiting.pos[0] - 1.0).abs() < 1e-5);
    assert!((waiting.pos[1] - 13.0).abs() < 1e-5);
    assert_eq!(waiting.rot[0], 120.0);
}

#[test]
fn missing_stop_trigger_is_reported_and_helper_geometry_is_omitted() {
    let f = Fixture::new();
    setup(&f);
    objects(
        &f,
        &[
            ("stop.000", [301.0, 10.0, 7.0]),
            ("stop_house", [301.0, 10.0, 7.0]),
        ],
    );
    config(
        &f,
        "busstops",
        "stop.txt",
        "[busstop]\nname=Stop\n[from_3d]\nreadFrom3D=1\nprefix=stop\n",
    );
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.stops_imported, 0);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("missing bus-stop trigger")));
    assert_eq!(
        report.triangles, 3,
        "unrecognised prefixed helpers keep the previous omission policy"
    );
}

#[test]
fn zero_duration_cycles_and_non_finite_gameplay_parameters_are_rejected_safely() {
    let f = Fixture::new();
    setup(&f);
    config(
        &f,
        "trafficlights",
        "zero.txt",
        "[trafficlight]\nprefix=zero\nhowManyTicks=1\n[tick1]\nrepeat=0\n",
    );
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.signals_imported, 0);
    assert!(report.warnings.iter().any(|w| w.contains("zero-duration")));
    let f = Fixture::new();
    setup(&f);
    config(&f, "busstops", "stop.txt", "[busstop]\npaxAmount=NaN\n");
    assert!(import(&f.0, "maps/Proton-Town", || false)
        .unwrap_err()
        .to_string()
        .contains("invalid Proton parameter"));
}

#[test]
fn emissive_alpha_modes_keep_zero_depth_writes_for_additive_approximations() {
    let f = Fixture::new();
    setup(&f);
    objects(
        &f,
        &[
            ("screen_emissive_transparent_", [301.0, 10.0, 7.0]),
            ("glow_additive_", [301.0, 10.0, 7.0]),
        ],
    );
    import(&f.0, "maps/Proton-Town", || false).unwrap();
    let material: Vec<_> = loaded(&f)
        .iter()
        .flat_map(|t| &t.objects)
        .filter(|o| o.file.contains("/tile_"))
        .flat_map(|o| {
            sco(&f, o)
                .model
                .meshes
                .into_iter()
                .flat_map(|m| m.materials)
        })
        .collect();
    assert!(
        material
            .iter()
            .any(|m| m.alpha == 2 && m.no_z_write && m.allcolor.is_some()),
        "{material:?}"
    );
    assert!(material
        .iter()
        .any(|m| m.alpha == 2 && !m.no_z_write && m.allcolor.is_some()));
}

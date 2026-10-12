use crate::{
    import,
    navigation::Navigation,
    tests::{chunk, named_model, setup, Fixture},
};

fn add_objects(f: &Fixture, names: &[&str]) {
    let mut edit = Vec::new();
    for name in names {
        edit.extend_from_slice(&named_model(name)[12..]);
    }
    std::fs::write(
        f.0.join("Town/tiles/0x0/road.3ds"),
        chunk(0x4d4d, &chunk(0x3d3d, &edit)),
    )
    .unwrap();
}

fn config(f: &Fixture, folder: &str, name: &str, contents: &str) {
    let dir = f.0.join("Town/tiles/0x0").join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), contents).unwrap();
}

fn nav(f: &Fixture) -> Navigation {
    Navigation::from_bytes(&omsi_cfg::vfs::read(&f.0.join(crate::navigation::FILE)).unwrap()).unwrap()
}

#[test]
fn phase_31_gps_roads_and_night_lights_survive_conversion_without_helper_meshes() {
    let f = Fixture::new();
    setup(&f);
    std::fs::write(
        f.0.join("Town.map.txt"),
        "[map]\nbaseDir=Town\nmodelsDir=0x0\nmapModVersion=3.1\n\n[openomsi]\nallowConversion=1\n",
    )
    .unwrap();
    add_objects(
        &f,
        &[
            "road",
            "route_gps_Start_part2",
            "pole_sodium_real_01",
            "pole_sodium_fake_01",
        ],
    );
    config(&f,"aivehicles","main.txt","[1]\nposX=299\nposY=7\nposZ=10\n[2]\nposX=310\nposY=8\nposZ=22\n[automatic_setup]\nreverse=1\nloop=1\n");
    config(&f,"streetlights","lamp.txt","[streetlight]\nprefix=sodium\n[real]\ncolorR=1\ncolorG=0.78\ncolorB=0.5\nrange=30\nintensity=0.9\n[fake]\ntexture=GLOW.PNG\ntexScaleX=6\ntexScaleY=6\ncolorA=0.4\n");
    std::fs::write(f.0.join("Town/textures/glow.png"), [0]).unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(
        (
            report.gps_imported,
            report.roads_imported,
            report.lights_imported
        ),
        (1, 1, 2)
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        report.triangles, 3,
        "only the road crosses the tile boundary"
    );
    let data = nav(&f);
    assert_eq!(data.routes[0].name, "Start");
    assert_eq!(
        data.routes[0].triangles,
        vec![[[299.0, 10.0], [301.0, 10.0], [299.0, 12.0]]]
    );
    assert_eq!(
        data.roads[0].points,
        vec![[310.0, 22.0, 8.0], [299.0, 10.0, 7.0], [310.0, 22.0, 8.0]]
    );
    let sco = omsi_scenery::SceneryObject::load(&f.0.join("proton-import/tile_1_0.oscoi")).unwrap();
    assert_eq!(sco.map_lights.len(), 1);
    assert_eq!(sco.map_lights[0].pos, [0.0, 11.0, 7.0]);
    assert_eq!(sco.map_lights[0].radius, 5.0);
    assert!((sco.map_lights[0].color[1] - 0.78 * 0.9).abs() < 1e-6);
    let glow = &sco.model.meshes[0].light_enh[0];
    assert_eq!(glow.variable, "NightlightA");
    assert_eq!(glow.pos, [0.0, 11.0, 7.0]);
    let texture = glow.texture.as_deref().expect("imported glow texture");
    // Case-insensitive filesystems may preserve the spelling from the config.
    assert!(texture.eq_ignore_ascii_case("glow.png"));
    assert_eq!(glow.size, 6.0);
    assert_eq!(glow.values[0], 0.4);
    assert_eq!(glow.values[2], 2.0, "no additional fog halo");
    assert_eq!(
        omsi_cfg::vfs::read(&f.0.join("proton-import/texture").join(texture)).unwrap(),
        [0],
        "the emitted texture reference must resolve to the copied source"
    );
}

#[test]
fn numbered_3d_road_markers_remain_disconnected_at_gaps_and_keep_lookalikes() {
    let f = Fixture::new();
    setup(&f);
    add_objects(
        &f,
        &[
            "road",
            "road_a.000",
            "road_a.001",
            "road_a.003",
            "road_a.004",
            "road_a.01",
            "road_ab.000",
        ],
    );
    config(
        &f,
        "aivehicles",
        "road.txt",
        "[from_3d]\nreadFrom3D=1\nprefix=road_a\n[automatic_setup]\nloop=1\n",
    );
    // The artificial boxes have the same centre; make successive markers distinct.
    let file = f.0.join("Town/tiles/0x0/road.3ds");
    let mut model = crate::three_ds::parse(&omsi_cfg::vfs::read(&file).unwrap()).unwrap();
    let mut edit = Vec::new();
    for (i, object) in model.objects.iter_mut().enumerate() {
        let mut positions = (object.positions.len() as u16).to_le_bytes().to_vec();
        for p in &mut object.positions {
            p.x += i as f32;
            positions.extend(p.to_array().into_iter().flat_map(f32::to_le_bytes));
        }
        let faces = [1u16, 0, 1, 2, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut mesh = chunk(0x4110, &positions);
        mesh.extend(chunk(0x4120, &faces));
        let mut name = object.name.as_bytes().to_vec();
        name.push(0);
        name.extend(chunk(0x4100, &mesh));
        edit.extend(chunk(0x4000, &name));
    }
    std::fs::write(file, chunk(0x4d4d, &chunk(0x3d3d, &edit))).unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.markers_skipped, 4);
    assert_eq!(report.roads_imported, 2);
    assert!(report.warnings.iter().any(|w| w.contains("disconnected")));
    let data = nav(&f);
    assert_eq!(
        data.roads[0].points,
        vec![[301.0, 11.0, 7.0], [302.0, 11.0, 7.0]]
    );
    assert_eq!(
        data.roads[1].points,
        vec![[303.0, 11.0, 7.0], [304.0, 11.0, 7.0]]
    );
    assert_eq!(
        report.triangles, 5,
        "lookalikes retain their visible triangles"
    );
}

#[test]
fn gps_route_names_are_exact_and_an_unmatched_route_does_not_leak_into_other_entries() {
    let f = Fixture::new();
    setup(&f);
    std::fs::write(f.0.join("Town/tiles/0x0/entrypoints.txt"),"[entrypoint_1]\nname=101 Downtown\nposX=299.5\nposZ=10.5\nposY=7\n[entrypoint_2]\nname=101\nposX=299.5\nposZ=10.5\nposY=7\n").unwrap();
    add_objects(
        &f,
        &[
            "road",
            "route_gps_101 Downtown_part1",
            "route_gps_101_part1",
            "route_gps_1010_part1",
            "route_gps_unknown_part1",
        ],
    );
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!((report.gps_imported, report.gps_skipped), (2, 2));
    let data = nav(&f);
    assert!(data.routes.iter().all(|r| r.triangles.len() == 1));
    assert_eq!(report.triangles, 3);
    assert!(Navigation::from_bytes(br#"{"version":2,"routes":[],"roads":[]}"#).is_err());
    assert!(Navigation::from_bytes(
        br#"{"version":1,"routes":[],"roads":[{"name":"bad","points":[[1000001,0,0],[0,0,0]]}]}"#
    )
    .is_err());
}

#[test]
fn normalized_gps_tags_choose_longest_route_and_explicit_unknown_does_not_use_file_route() {
    let f = Fixture::new(); setup(&f);
    std::fs::write(f.0.join("Town/tiles/0x0/entrypoints.txt"),"[entrypoint_1]\nname=101 Downtown\nposX=299.5\nposZ=10.5\nposY=7\n[entrypoint_2]\nname=101\nposX=299.5\nposZ=10.5\nposY=7\n").unwrap();
    add_objects(&f,&["road","GPS_101_DOWNTOWN-part1","route_gps_1010_part1"]);
    let file=f.0.join("Town/tiles/0x0/GPS-101-Downtown.3ds");
    std::fs::write(&file,named_model("route_gps_unknown_part1")).unwrap();
    let report=import(&f.0,"maps/Proton-Town",||false).unwrap();
    let data=nav(&f);
    assert_eq!(data.routes[0].triangles.len(),1);
    assert!(data.routes[1].triangles.is_empty());
    assert_eq!(report.gps_skipped,2);
    assert_eq!(data.route_index("101_DOWNTOWN"),Some(0));
    assert_eq!(data.route_index("1010"),None);
}

#[test]
fn disabled_manual_links_keep_overview_but_never_generate_ai_paths_or_fake_origins() {
    let f=Fixture::new();setup(&f);
    config(&f,"aivehicles","manual.txt","[automatic_setup]\nenabled=0\nloop=1\n[1]\nposX=299\nposY=7\nposZ=10\n[2]\nposX=310\nposY=7\nposZ=20\n[3]\nisSpawner=0\n");
    let report=import(&f.0,"maps/Proton-Town",||false).unwrap();
    let data=nav(&f);
    assert_eq!(report.traffic_segments_imported,0);
    assert_eq!(data.roads.len(),1);assert!(data.roads[0].overview_only);
    assert_eq!(data.roads[0].points.len(),2);
    assert!(report.warnings.iter().any(|w|w.contains("overview only")));
    assert!(!data.road_triangles.is_empty());
}

#[test]
fn authored_splines_expand_shared_navigation_and_traffic_with_rules_preserved() {
    let f=Fixture::new();setup(&f);
    let file=f.0.join("Town.map.txt");let mut cfg=std::fs::read_to_string(&file).unwrap();cfg.push_str("\n[ai_traffic]\nuseSplines=1\n");std::fs::write(file,cfg).unwrap();
    config(&f,"aivehicles","smooth.txt","[defaults]\nisBusSpawner=1\nisSpawner=0\n[1]\nposX=299\nposY=7\nposZ=10\n[2]\nposX=339\nposY=9\nposZ=10\n[3]\nposX=379\nposY=11\nposZ=20\n");
    let report=import(&f.0,"maps/Proton-Town",||false).unwrap();
    let data=nav(&f);assert!(data.roads[0].points.len()>3);assert!(!data.roads[0].overview_only);
    assert!(data.roads[0].points.contains(&[339.,10.,9.]));
    assert!(report.traffic_segments_imported>2);
}

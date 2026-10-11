use super::*;

#[test]
fn generated_stop_lines_enter_the_native_network_and_obey_independent_signal_channels() {
    let cfg = omsi_cfg::CfgFile::from_str("paths.sco", "[absheight]\n[fixed]\n[trafficlight]\n[traffic_lights_group]\n10\n[traffic_light]\nred_picture\n[phase]\n0\n4\n[phase]\n12\n6\n[traffic_light]\nstop_trigger\n[phase]\n0\n4\n[phase]\n12\n6\n[path]\n5\n10\n7\n90\n0\n10\n0.1\n0.1\n0\n3.5\n0\n3\n[use_traffic_light]\n1\n");
    let sco = SceneryObject::parse(&cfg);
    assert!(traffic_light_program_enabled(&sco, false));
    let mut lanes = vec![LaneBuilder::arc(
        DVec3::new(292.0, 10.0, 7.0),
        90.0,
        13.0,
        0.0,
        0.0,
        LaneKind::Street,
        3.5,
    )];
    lanes.extend(object_lanes(
        &sco,
        DVec3::new(300.0, 0.0, 0.0),
        [0.0; 3],
        Some(0),
        (1, 0),
        1_000_000,
        &[],
    ));
    let mut net = omsi_sim::traffic::Network {
        lanes,
        ..Default::default()
    };
    net.link(omsi_sim::traffic::LINK_TOLERANCE);
    assert_eq!(net.lanes[0].next, [1]);
    let controlled = &net.lanes[1];
    assert_eq!(controlled.traffic_light, Some((0, 1)));
    assert_eq!(controlled.turn, 2);
    assert!((controlled.start() - DVec3::new(305.0, 10.0, 7.0)).length() < 1e-5);
    assert!((controlled.end() - DVec3::new(315.0, 10.0, 8.0)).length() < 1e-5);
    let phases = sco
        .traffic_lights
        .iter()
        .map(|l| {
            (
                l.phases.iter().map(|p| (p.state, p.duration)).collect(),
                l.approach_dist,
            )
        })
        .collect();
    let mut controller =
        TrafficLightController::from_program(phases, sco.traffic_lights_group, &[], &[]);
    assert!(!TrafficLightController::allows_go(controller.state(1)));
    controller.advance(4.0);
    assert!(TrafficLightController::allows_go(controller.state(1)));
}

/// Generated real-map data stays outside the repository. This checks the native
/// lane adapter and all lamp-parent references, without allocating a scene's GPU mesh.
#[test]
fn supplied_generated_gameplay_when_available() {
    let Some(dir) = std::env::var_os("PROTON_GENERATED_MAP").map(PathBuf::from) else {
        return;
    };
    let global = GlobalCfg::load(&dir.join("global.cfg")).unwrap();
    let mut controlled = 0;
    let mut sidewalks = 0;
    let mut stops = 0;
    let mut lamps = 0;
    for t in global.tiles {
        let tile = omsi_map::Tile::load(&dir.join(t.file)).unwrap();
        if std::env::var_os("PROTON_VERIFY_GEOMETRY").is_some() {
            let file = dir.join(format!("proton-import/model/tile_{}_{}.o3d", t.x, t.y));
            let mesh = omsi_o3d::load_mesh(&file).unwrap();
            assert!(mesh
                .vertices
                .iter()
                .all(|v| v.position.is_finite() && v.normal.is_finite() && v.uv.is_finite()));
            assert!(mesh.triangles.iter().all(|t| t
                .indices
                .iter()
                .all(|i| (*i as usize) < mesh.vertices.len())
                && (t.material as usize) < mesh.materials.len()));
        }
        let mut controllers = hashbrown::HashSet::new();
        let mut children = Vec::new();
        for object in tile.objects {
            let Some(rel) = object.file.split("/proton-import/").nth(1) else {
                continue;
            };
            let sco = SceneryObject::load(&dir.join("proton-import").join(rel)).unwrap();
            stops += usize::from(sco.is_bus_stop);
            if !sco.traffic_lights.is_empty() {
                controllers.insert(object.id);
            }
            if let Some(parent) = object.var_parent {
                children.push(parent);
                lamps += 1;
            }
            let origin =
                DVec3::new(t.x as f64 * 300.0, t.y as f64 * 300.0, 0.0) + DVec3::from(object.pos);
            for lane in object_lanes(
                &sco,
                origin,
                object.rot,
                (!sco.traffic_lights.is_empty()).then_some(0),
                (t.x, t.y),
                object.id,
                &object.rules,
            ) {
                assert!(lane.length().is_finite() && lane.length() > 0.01);
                assert!(lane.points.iter().all(|p| p.is_finite()));
                controlled += usize::from(lane.traffic_light.is_some());
                sidewalks += usize::from(lane.kind == LaneKind::Sidewalk);
            }
        }
        assert!(
            children.iter().all(|p| controllers.contains(p)),
            "lamp parent missing from owner tile"
        );
    }
    assert!(stops > 0 && sidewalks > 0 && controlled > 0 && lamps > 0);
    eprintln!("native generated gameplay: {stops} stops, {sidewalks} sidewalk segments, {controlled} controlled lanes, {lamps} lamp objects");
}

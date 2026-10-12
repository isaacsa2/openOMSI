use crate::{
    tests::{chunk, Fixture},
    vehicle_package::convert,
};
use std::{path::Path, sync::Arc};

pub(crate) fn local_model() -> Vec<u8> {
    let mut objects = Vec::new();
    for (name, p) in [
        ("body_ecam_", [0., 0., 1.]),
        ("_wheel_fl_", [-1., 3., 0.5]),
        ("_wheel_fr_", [1., 3., 0.5]),
        ("_wheel_rl_", [-1., -3., 0.5]),
        ("_wheel_rr_", [1., -3., 0.5]),
        ("_rdoor_1x1_", [1., 4., 1.]),
        ("_sit_", [0.6, 0., 1.]),
        ("_stand_", [-0.6, 0., 1.]),
        ("_r1b1_", [1.4, 4., 0.5]),
        ("_r1b2_", [0., 4., 0.5]),
        ("_r1b3_", [0., 0., 0.5]),
        ("_r1z1_", [0., 0., 0.5]),
        ("_r1z2_", [1.4, 4., 0.5]),
        ("_wipers1_00_", [0., 5., 2.]),
        ("_wipers1_01_", [0., 5., 2.]),
        ("_display_front_", [0., 5., 2.5]),
        ("_mirror1_", [-1., 4., 2.]),
    ] {
        let mut verts = 3u16.to_le_bytes().to_vec();
        for v in [
            [p[0] - 0.1f32, p[1] - 0.1, p[2]],
            [p[0] + 0.1, p[1] - 0.1, p[2]],
            [p[0], p[1] + 0.1, p[2]],
        ] {
            for n in v {
                verts.extend(n.to_le_bytes());
            }
        }
        let faces = [1u16, 0, 1, 2, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut geo = chunk(0x4110, &verts);
        geo.extend(chunk(0x4120, &faces));
        let mut obj = name.as_bytes().to_vec();
        obj.push(0);
        obj.extend(chunk(0x4100, &geo));
        objects.extend(chunk(0x4000, &obj));
    }
    chunk(0x4d4d, &chunk(0x3d3d, &objects))
}
fn fixture(f: &Fixture, permission: &str) -> std::path::PathBuf {
    let base = f.0.join("source/Bus");
    std::fs::create_dir_all(base.join("Textures")).unwrap();
    let main = f.0.join("source/Bus.ini");
    std::fs::write(&main,format!("[mod]\nname=Fixture\nbusModVersion=2\n[config]\nbaseDir=Bus\nmodels1=models.txt\nwheels1=wheels.txt\nengine1auto=engine.txt\nsounds1=sounds.txt\nlights1=lights.txt\n[folders]\ntextures=Textures\n[data]\nmass1=12000\ntemplate=4x2\n[collision1]\nsizeX=2.5\nsizeY=3\nsizeZ=12\nposY=1.5\n[posCenterOfMass1]\nposY=1\n[posDriverHead]\nposX=-0.5\nposY=2\nposZ=4\n[leftMirror]\nposX=-1\nposY=2\nposZ=4\n[doors]\nrightdoor1=door.txt\n{permission}" )).unwrap();
    std::fs::write(base.join("models.txt"), "[part1_3ds]\nmodel1=body.3ds\n").unwrap();
    std::fs::write(base.join("body.3ds"), local_model()).unwrap();
    std::fs::write(base.join("wheels.txt"),"[leftwheel1]\nposX=-1\nposY=0.5\nposZ=3\nradius=0.5\n[rightwheel1]\nposX=1\nposY=0.5\nposZ=3\nradius=0.5\n[leftwheel2]\nposX=-1\nposY=0.5\nposZ=-3\nradius=0.5\n[rightwheel2]\nposX=1\nposY=0.5\nposZ=-3\nradius=0.5\n").unwrap();
    std::fs::write(base.join("engine.txt"),"[engine]\npeak_rpm_torque=900\n[automatic_gearbox]\nnum_forward_ratios=3\nfwd_ratio_1=5\nfwd_ratio_2=3\nfwd_ratio_3=1\n").unwrap();
    std::fs::write(base.join("door.txt"),"[rightdoor1]\nisEntrance=1\nisExit=1\nanim_rotate_1=-90\n[anim1_door1]\nposX=1\nposY=2\nposZ=4\n").unwrap();
    std::fs::write(
        base.join("sounds.txt"),
        "[sound1]\nfile=engine.wav\nvolumeMultiplier=1\n[horn]\nfile=horn.wav\nvolume=1\n",
    )
    .unwrap();
    std::fs::write(base.join("engine.wav"), b"synthetic-engine-audio").unwrap();
    std::fs::write(base.join("horn.wav"), b"synthetic-horn-audio").unwrap();
    std::fs::write(base.join("lights.txt"),"[low_beam_spotlight_1]\nposX=1\nposY=1\nposZ=5\nrange=25\nspotAngle=76\n[interior_light_1]\nposY=2\nposZ=2\n").unwrap();
    std::fs::write(
        f.0.join("source/LICENSE.txt"),
        b"fixture attribution, keep exactly\r\n",
    )
    .unwrap();
    main
}
fn native(root: &Path) -> omsi_sim::VehicleInstance {
    let file = root.join("Vehicles/Proton-Fixture/vehicle.busoi");
    let ty = omsi_sim::VehicleType::load(root, &file).unwrap();
    assert!(ty.missing_packs.is_empty());
    assert_eq!(ty.meshes.len(), 10);
    omsi_sim::VehicleInstance::new(Arc::new(ty), omsi_sim::VehicleHost::new(Default::default()))
}
#[test]
fn denied_vehicle_conversion_never_writes_partial_files_or_declaration() {
    for declaration in [
        "",
        "[openomsi]\nallowConversion=0\n",
        "[openomsi]\nallowConversion=\n",
        "[openomsi]\nallowConversion=true\n",
        "[openomsi]\nallowConversion=1\nallowConversion=0\n",
    ] {
        let f = Fixture::new();
        let file = fixture(&f, declaration);
        let before = std::fs::read(&file).unwrap();
        let out = f.0.join("output");
        assert!(convert(&file, &out, || false)
            .unwrap_err()
            .to_string()
            .contains("conversion blocked"));
        assert!(!out.exists());
        assert_eq!(std::fs::read(file).unwrap(), before);
        assert!(!std::fs::read_dir(&f.0).unwrap().flatten().any(|p| p
            .file_name()
            .to_string_lossy()
            .starts_with(".proton-vehicle")));
    }
}
#[test]
fn conversion_preserves_local_geometry_sources_and_native_vehicle_systems() {
    let f = Fixture::new();
    let main = fixture(&f, "[openomsi]\nallowConversion=1\n");
    let original = std::fs::read(&main).unwrap();
    let out = f.0.join("output");
    let r = convert(&main, &out, || false).unwrap();
    assert_eq!((r.seats, r.standing, r.doors), (1, 1, 1));
    assert_eq!(r.sounds, 2);
    assert_eq!(std::fs::read(&main).unwrap(), original);
    let base = out.join("Vehicles/Proton-Fixture");
    assert_eq!(
        std::fs::read(base.join("Source/LICENSE.txt")).unwrap(),
        b"fixture attribution, keep exactly\r\n"
    );
    let def = omsi_vehicle::Vehicle::load(&base.join("vehicle.busoi")).unwrap();
    assert_eq!(def.mass, 12.);
    assert!(matches!(def.kind, omsi_vehicle::vehicle::VehicleKind::Bus));
    assert_eq!(def.axles.len(), 2);
    assert!(!def.axles[0].driven && def.axles[1].driven);
    assert_eq!(def.cameras_driver[0].pos, [-0.5, 4., 2.]);
    assert_eq!(
        omsi_vehicle::vehicle::offered_vehicles(&[base.join("vehicle.busoi")]).len(),
        1
    );
    for i in 0..r.meshes {
        let mesh = omsi_o3d::load_mesh(&base.join(format!("Model/mesh_{i}.o3doi"))).unwrap();
        assert!(mesh
            .vertices
            .iter()
            .all(|v| v.position.abs().max_element() < 10.));
    }
    let mut v = native(&out);
    assert_eq!(v.ty.model.spotlights_2[0].values[10..], [20., 76.]);
    assert_eq!(v.ty.model.interior_lights.len(), 1);
    assert_eq!(v.ty.def.cameras_reflexion.len(), 1);
    assert_eq!(v.ty.model.meshes[0].viewpoint, 5);
    assert!(v
        .ty
        .meshes
        .iter()
        .any(|m| m.materials.iter().any(|m| m.texture == "reflexion0.bmp")));
    assert!(v
        .ty
        .model
        .meshes
        .iter()
        .any(|m| m.materials.iter().any(|m| m.use_text_texture == Some(0))));
    assert!(v.ty.program.str_var("IBIS_terminus_name").is_some());
    let sound = omsi_vehicle::SoundCfg::load(&base.join("Sound/sound.cfgoi")).unwrap();
    assert_eq!(sound.sounds.len(), 2);
    assert!(sound
        .sounds
        .iter()
        .any(|s| s.conditions.iter().any(|c| c.variable == "horn")));
    assert!(v.trigger("kw_m_enginestart"));
    assert!(v.trigger("parking_brake_release"));
    assert!(v.trigger("automatic_D"));
    v.set_controls(omsi_sim::Controls {
        throttle: 0.7,
        steering: 0.3,
        ..Default::default()
    });
    for _ in 0..400 {
        v.update(0.02);
    }
    assert!(
        v.physics.speed > 2. && v.physics.speed < 40.,
        "speed {}",
        v.physics.speed
    );
    assert!(v.heading.abs() > 1.);
    assert!(v.var("engine_n").unwrap().is_finite());
    v.set_controls(omsi_sim::Controls {
        brake: 1.,
        ..Default::default()
    });
    for _ in 0..400 {
        v.update(0.02);
    }
    assert!(v.physics.speed.abs() < 0.1);
    assert!(v.trigger("bus_doorfront0"));
    for _ in 0..70 {
        v.update(0.02);
    }
    assert_eq!(v.pax_open("Entry", 0), Some(1.));
    assert_eq!(v.pax_open("Exit", 0), Some(1.));
    assert!(v.var("oi_door_0").unwrap() > 0.99);
    assert!(v.trigger("bus_doorfront0"));
    v.set_pax_var("Entry", 0, "Busy", 1.);
    for _ in 0..70 {
        v.update(0.02);
    }
    assert_eq!(v.pax_open("Entry", 0), Some(1.));
    v.set_pax_var("Entry", 0, "Busy", 0.);
    assert!(v.trigger("bus_doorfront0"));
    for _ in 0..70 {
        v.update(0.02);
    }
    assert_eq!(v.pax_open("Entry", 0), Some(0.));
    assert!(v.trigger("automatic_R"));
    v.set_controls(omsi_sim::Controls {
        throttle: 0.5,
        ..Default::default()
    });
    for _ in 0..100 {
        v.update(0.02);
    }
    assert!(v.physics.speed < -0.5);
    v.trigger("automatic_D");
    assert_eq!(v.var("oi_selector"), Some(-1.));
    // The in-game player uses rigid wheel/ground contacts, not only simple dynamics.
    let mut rigid = native(&out);
    rigid.ground = Some(Arc::new(|_, _| Some(0.)));
    rigid.enable_rigid_body();
    rigid.trigger("kw_m_enginestart");
    rigid.trigger("parking_brake_release");
    rigid.trigger("automatic_D");
    rigid.set_controls(omsi_sim::Controls {
        throttle: 0.5,
        ..Default::default()
    });
    for _ in 0..200 {
        rigid.update(0.02);
    }
    assert!(rigid.position.is_finite() && rigid.physics.speed.is_finite());
    assert!(rigid.physics.speed > 0.5);
}
#[test]
fn failed_or_cancelled_vehicle_conversion_discards_stage() {
    let f = Fixture::new();
    let main = fixture(&f, "[openomsi]\nallowConversion=1\n");
    let out = f.0.join("output");
    let n = std::sync::atomic::AtomicUsize::new(0);
    assert!(convert(&main, &out, || n
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        > 20)
    .is_err());
    assert!(!out.exists());
    std::fs::write(
        f.0.join("source/Bus/models.txt"),
        "[part1_3ds]\nmodel1=body.3ds\nmodel2=bad.3ds\n",
    )
    .unwrap();
    std::fs::write(f.0.join("source/Bus/bad.3ds"), b"damaged").unwrap();
    assert!(convert(&main, &out, || false).is_err());
    assert!(!out.exists());
    assert!(!std::fs::read_dir(&f.0).unwrap().flatten().any(|p| p
        .file_name()
        .to_string_lossy()
        .starts_with(".proton-vehicle")));
}

#[test]
fn supplied_vehicle_definitions_can_be_inspected_without_a_conversion_or_source_write() {
    let Some(file) = std::env::var_os("PROTON_VEHICLE_FIXTURE") else {
        return;
    };
    let file = std::path::PathBuf::from(file);
    let before = std::fs::read(&file).unwrap();
    let info = crate::vehicle::inspect(&file).unwrap();
    let physics = crate::vehicle_physics::Physics::read(&info).unwrap();
    let cfg = crate::ini(&file).unwrap();
    physics.definition(&cfg, &info).unwrap();
    let mut cabin = crate::vehicle_cabin::Cabin::default();
    let mut objects = 0;
    for model in &info.models {
        crate::three_ds::Scene::open(model, &|| false)
            .unwrap()
            .visit_import(&|| false, |o, _| {
                cabin.marker(&o)?;
                objects += 1;
                Ok(())
            })
            .unwrap();
    }
    let mut report = crate::vehicle_package::Report::default();
    cabin.definitions(&info, &cfg, &mut report).unwrap();
    assert!(!physics.axles.is_empty() && objects > 0);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    eprintln!(
        "read only: {} models, {} objects, {} axles, {} doors, {} seats/{} standing",
        info.models.len(),
        objects,
        physics.axles.len(),
        info.doors.len(),
        report.seats,
        report.standing
    );
}

#[test]
fn a_converted_real_map_can_be_read_by_native_parsers() {
    let Some(root) = std::env::var_os("PROTON_CONVERTED_PACKAGE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let map = root.join("maps/Proton-Grande Recife");
    let global = omsi_map::GlobalCfg::load(&map.join(omsi_cfg::oi::GLOBAL)).unwrap();
    assert!(!global.tiles.is_empty() && !global.entry_points.is_empty());
    let navigation = crate::navigation::Navigation::from_bytes(
        &omsi_cfg::vfs::read(&map.join(crate::navigation::FILE)).unwrap(),
    )
    .unwrap();
    assert!(!navigation.routes.is_empty());
    let hof = omsi_vehicle::Hof::load(&map.join(crate::depot::FILE)).unwrap();
    assert!(!hof.termini.is_empty());
    let mut triangles = 0;
    for tile in &global.tiles {
        let tile = omsi_map::Tile::load(&map.join(&tile.file)).unwrap();
        for object in tile.objects {
            let path = root.join(object.file);
            let sco = omsi_scenery::SceneryObject::load(&path).unwrap();
            for model in sco
                .model
                .meshes
                .iter()
                .map(|m| m.file.as_str())
                .chain(sco.collision_mesh.as_deref())
            {
                let mesh =
                    omsi_o3d::load_mesh(&path.parent().unwrap().join("model").join(model)).unwrap();
                assert!(mesh
                    .vertices
                    .iter()
                    .all(|v| v.position.is_finite() && v.normal.is_finite()));
                triangles += mesh.triangles.len();
            }
        }
    }
    assert!(triangles > 0);
    eprintln!(
        "native readers: {} tiles, {} triangles, {} GPS groups, {} termini",
        global.tiles.len(),
        triangles,
        navigation.routes.len(),
        hof.termini.len()
    );
}

use super::*;

#[test]
fn authored_nested_bases_are_separate_and_unsafe_paths_are_rejected() {
    let entries: Vec<_> = [
        ("Pack/Phase3.map.txt", Some("./Shared\\Old/")),
        ("Pack/Phase4.map.txt", Some("Shared/New")),
        ("Pack/Shared/Old/tiles/x/road.3ds", None),
        ("Pack/Shared/New/tiles/x/road.3ds", None),
        ("Pack/Shared/unrelated.dat", None),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (rel, base))| Entry {
        rel: rel.into(),
        size: 1,
        index,
        proton_base: base.map(str::to_owned),
    })
    .collect();
    let mut tree = Node::default();
    for (i, e) in entries.iter().enumerate() {
        tree.insert(&e.rel, i);
    }
    let node = tree.get("Pack").unwrap();
    let mut plan = Plan::default();
    let mut used = HashSet::new();
    assert!(plan_at(node, "Pack", &entries, &mut plan, &mut used));
    assert_eq!(plan.maps.len(), 2);
    assert_eq!(used.len(), 4);
    assert_eq!(
        plan.maps[0].files,
        [
            (2, "Shared/Old/tiles/x/road.3ds".into()),
            (0, "Phase3.map.txt".into())
        ]
    );
    assert_eq!(
        plan.maps[1].files,
        [
            (3, "Shared/New/tiles/x/road.3ds".into()),
            (1, "Phase4.map.txt".into())
        ]
    );
    for path in [
        "../Shared/Old",
        "/Shared/Old",
        "C:/Shared/Old",
        "Shared/Missing",
    ] {
        assert!(folder(node, path).is_none());
    }
    assert_eq!(folder(node, "shared/oLD").unwrap().0, "Shared/Old");
    let mut case = Node::default();
    case.insert("Base/tiles/x/A.3ds", 0);
    case.insert("base/tiles/x/B.3ds", 1);
    assert_eq!(folder(&case, "base").unwrap().0, "base");
}

#[test]
fn reads_bounded_configs_from_zip_and_folder_before_planning() {
    let root = std::env::temp_dir().join(format!("proton-layout-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("Display.map.txt");
    let text = "\u{feff}[map]\nbaseDir=Wrapper/Base\nmapModVersion=4\n";
    std::fs::write(&config, text).unwrap();
    let archive = root.join("pack.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    zip.start_file("Display.map.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(text.as_bytes()).unwrap();
    zip.finish().unwrap();
    let job = Job {
        id: 0,
        source: root.clone(),
        mode: InstallMode::Extract,
        from_inbox: false,
        cancel: AtomicBool::new(false),
        progress: Mutex::new(Progress::default()),
        bytes_done: AtomicU64::new(0),
        files_done: AtomicU64::new(0),
        linked: Mutex::new(Vec::new()),
        stall_at: u64::MAX,
    };
    for mut source in [
        Source::Folder(vec![config]),
        Source::Zip(
            zip::ZipArchive::new(std::io::BufReader::new(
                std::fs::File::open(&archive).unwrap(),
            ))
            .unwrap(),
        ),
    ] {
        let mut entries = [Entry {
            rel: "Display.map.txt".into(),
            size: text.len() as u64,
            index: 0,
            proton_base: None,
        }];
        read_layouts(&mut source, &mut entries, &job).unwrap();
        assert_eq!(entries[0].proton_base.as_deref(), Some("Wrapper/Base"));
        entries[0].size = 64 * 1024 + 1;
        entries[0].proton_base = None;
        read_layouts(&mut source, &mut entries, &job).unwrap();
        assert!(entries[0].proton_base.is_none());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn standalone_maps_and_multiple_siblings_are_planned_separately() {
    let entries: Vec<_> = [
        "Pack/A.map.txt",
        "Pack/A/tiles/x/map.3ds",
        "Pack/A/textures/a.png",
        "Pack/B.map.txt",
        "Pack/B/tiles/x/map.3ds",
    ]
    .iter()
    .enumerate()
    .map(|(index, rel)| Entry {
        rel: rel.to_string(),
        size: 1,
        index,
        proton_base: None,
    })
    .collect();
    let mut tree = Node::default();
    for (i, e) in entries.iter().enumerate() {
        tree.insert(&e.rel, i);
    }
    let mut plan = Plan::default();
    let mut used = HashSet::new();
    assert!(plan_at(
        tree.get("Pack").unwrap(),
        "Pack",
        &entries,
        &mut plan,
        &mut used
    ));
    assert_eq!(plan.maps.len(), 2);
    assert_eq!(used.len(), entries.len());
    assert_eq!(plan.maps[0].dest, "maps/Proton-A");
    assert!(plan.maps[0]
        .files
        .iter()
        .any(|(_, p)| p == "A/tiles/x/map.3ds"));
    assert!(check_mode(&plan, InstallMode::InPlace).is_err());
    assert!(check_mode(&plan, InstallMode::Extract).is_ok());
}

#[test]
fn unique_base_folder_and_inline_maps_are_detected_without_guessing_ambiguous_layouts() {
    for paths in [
        vec!["Pack/Display.map.txt", "Pack/Actual/tiles/x/road.3ds"],
        vec!["Pack/Display.map.txt", "Pack/tiles/x/road.3ds"],
    ] {
        let entries: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(index, rel)| Entry {
                rel: rel.to_string(),
                size: 1,
                index,
                proton_base: None,
            })
            .collect();
        let mut tree = Node::default();
        for (i, e) in entries.iter().enumerate() {
            tree.insert(&e.rel, i);
        }
        let mut plan = Plan::default();
        let mut used = HashSet::new();
        assert!(plan_at(
            tree.get("Pack").unwrap(),
            "Pack",
            &entries,
            &mut plan,
            &mut used
        ));
        assert_eq!(plan.maps[0].dest, "maps/Proton-Display");
        assert_eq!(used.len(), entries.len());
        assert!(plan.maps[0]
            .files
            .iter()
            .any(|(_, rel)| rel == "Display.map.txt"));
    }
    let mut node = Node::default();
    node.insert("Display.map.txt", 0);
    node.insert("A/tiles/x/road.3ds", 1);
    node.insert("B/tiles/x/road.3ds", 2);
    assert!(!plan_at(
        &node,
        "",
        &[],
        &mut Plan::default(),
        &mut HashSet::new()
    ));
}

#[test]
fn existing_omsi_map_with_incidental_proton_text_is_not_converted() {
    let mut node = Node::default();
    node.insert("global.cfg", 0);
    node.insert("Other.map.txt", 1);
    node.insert("Other/tiles/x/map.3ds", 2);
    assert!(!plan_at(
        &node,
        "",
        &[],
        &mut Plan::default(),
        &mut HashSet::new()
    ));
}

#[test]
fn install_supplied_proton_zip_when_available() {
    let Some(src) = std::env::var_os("PROTON_INSTALL_FIXTURE") else {
        return;
    };
    let content = std::env::temp_dir().join(format!("proton-install-{}", std::process::id()));
    std::fs::create_dir_all(&content).unwrap();
    let report = run_blocking(
        content.clone(),
        None,
        PathBuf::from(src),
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(
        report.state, "done",
        "{}: {:?}",
        report.message, report.warnings
    );
    let expected: usize = std::env::var("PROTON_INSTALL_EXPECTED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
    let maps: Vec<_> = report.installed.iter().filter(|p| p.starts_with("maps/")).collect();
    assert_eq!(maps.len(), expected);
    assert!(report.installed.iter().any(|p| p == "HOFs"));
    for installed in maps {
        let map = content.join(installed);
        assert!(!map.join("proton-import/work").exists());
        let global = omsi_map::GlobalCfg::load(&map.join("global.cfg")).unwrap();
        assert!(!global.tiles.is_empty() && !global.entry_points.is_empty());
        let hof = omsi_vehicle::Hof::load(&content.join("HOFs").join(format!("{}.hof", omsi_proton::depot::name(installed)))).unwrap();
        assert_eq!(omsi_proton::depot::installed_name(&map), Some(hof.name.clone()));
        let info = crate::read_map(&map, map.file_name().unwrap().to_str().unwrap(), "en").unwrap();
        assert_eq!(info.hof, hof.name);
        eprintln!("{}: {} shared HOF destinations and {} stops", installed, hof.termini.len(), hof.bus_stops.len());
        for t in global.tiles {
            let tile = omsi_map::Tile::load(&map.join(t.file)).unwrap();
            for o in tile.objects {
                assert!(content.join(o.file).is_file());
            }
        }
    }
    std::fs::remove_dir_all(content).unwrap();
}

#[test]
fn generated_depot_is_published_and_owned_with_its_map_without_editing_buses() {
    let root = std::env::temp_dir().join(format!("proton-depot-install-{}", std::process::id()));
    let source = root.join("source");
    let content = root.join("content");
    let models = source.join("Town/tiles/x");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(source.join("Town/textures")).unwrap();
    std::fs::write(source.join("Town.map.txt"), "[map]\nbaseDir=Town\nmodelsDir=x\nmapModVersion=4\n").unwrap();
    std::fs::write(models.join("entrypoints.txt"), "[entrypoint_1]\nname=Centro\nposX=1\nposY=0\nposZ=1\n").unwrap();
    fn chunk(tag: u16, data: &[u8]) -> Vec<u8> {
        let mut out = tag.to_le_bytes().to_vec();
        out.extend(((data.len() + 6) as u32).to_le_bytes());
        out.extend(data);
        out
    }
    let mut positions = 3u16.to_le_bytes().to_vec();
    positions.extend([0f32, 0., 0., 10., 0., 0., 0., 10., 0.].into_iter().flat_map(f32::to_le_bytes));
    let mut mesh = chunk(0x4110, &positions);
    mesh.extend(chunk(0x4120, &[1u16, 0, 1, 2, 0].into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>()));
    let mut object = b"road\0".to_vec();
    object.extend(chunk(0x4100, &mesh));
    std::fs::write(models.join("road.3ds"), chunk(0x4d4d, &chunk(0x3d3d, &chunk(0x4000, &object)))).unwrap();
    std::fs::create_dir_all(content.join("Vehicles/Test")).unwrap();
    std::fs::write(content.join("Vehicles/Test/original.hof"), "unchanged").unwrap();
    std::fs::create_dir_all(content.join("HOFs")).unwrap();
    std::fs::write(content.join("HOFs/original.hof"), "unchanged shared").unwrap();
    let report = run_blocking(content.clone(), None, source, InstallMode::Extract, None, false);
    assert_eq!(report.state, "done", "{report:?}");
    assert_eq!(report.installed, ["maps/Proton-Town", "HOFs"]);
    let map = content.join("maps/Proton-Town");
    let hof = omsi_vehicle::Hof::load(&content.join("HOFs").join(format!("{}.hof", omsi_proton::depot::name("maps/Proton-Town")))).unwrap();
    assert_eq!(hof.termini[0].menu_name(), "Centro");
    assert_eq!(crate::read_map(&map, "Proton-Town", "en").unwrap().hof, hof.name);
    // Explicit native depot choices still take precedence over the generated hint.
    std::fs::write(map.join("ailists.cfg"), "[aigroup_depot]\nNative\nOriginal\n").unwrap();
    assert_eq!(crate::read_map(&map, "Proton-Town", "en").unwrap().hof, "Original");
    assert_eq!(std::fs::read_to_string(content.join("Vehicles/Test/original.hof")).unwrap(), "unchanged");
    assert!(!content.join(STAGING).exists());
    let registry: serde_json::Value = serde_json::from_slice(&std::fs::read(content.join("Mods").join(crate::mods::REGISTRY)).unwrap()).unwrap();
    let paths = registry["mods"][0]["paths"].as_array().unwrap();
    assert!(paths.iter().any(|p| p.as_str() == Some("maps/Proton-Town")));
    let owned_hof = format!("HOFs/{}.hof", omsi_proton::depot::name("maps/Proton-Town"));
    assert!(paths.iter().any(|p| p.as_str() == Some(&owned_hof)));
    let id = registry["mods"][0]["id"].as_str().unwrap();
    crate::mods::set_enabled(&content, id, false).unwrap();
    assert!(!map.exists() && !content.join(&owned_hof).exists());
    crate::mods::set_enabled(&content, id, true).unwrap();
    assert!(map.exists() && content.join(&owned_hof).exists());
    crate::mods::remove(&content, id).unwrap();
    assert!(!map.exists() && !content.join(&owned_hof).exists());
    assert_eq!(std::fs::read_to_string(content.join("HOFs/original.hof")).unwrap(), "unchanged shared");
    std::fs::remove_dir_all(root).unwrap();
}

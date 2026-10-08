include!("../../../../tools/test-support/original_root.rs");
use super::*;

/// The natural weather and the cycle need no file: a client takes them from any host.
#[test]
fn a_hosts_natural_weather_or_cycle_is_taken_without_a_file() {
    use clap::Parser;
    let args = crate::cli::Args::parse_from(["openomsi"]);
    for w in ["natural", "Natural", "cycle"] {
        assert_eq!(host_weather(&args, w), Ok(Some(w.to_string())), "{w}");
    }
    assert_eq!(host_weather(&args, ""), Ok(None));
    assert!(host_weather(&args, "weather/none_such.owt").is_err());
}

#[test]
fn variable_strings_cannot_replace_complete_unicode_fleet_metadata() {
    let mut vehicle = crate::schedule::tests::script_test_vehicle(
        "{frame}\n{end}\n", "", "ident\nnumber\nIBIS_Display\n");
    let fleet = "🚌".repeat(64);
    for name in ["ident", "number"] {
        let id = vehicle.ty.program.str_var(name).unwrap();
        vehicle.state.str_vars[id as usize] = fleet.clone();
    }
    let table = var_table(&vehicle.ty.program);
    let mut tx = omsi_net::vars::VarSender::default();
    let messages = tx.tick(omsi_net::PROTOCOL as u8, 2, table.hash,
        &[], &[], &table.strings, &[fleet.clone(), fleet.clone(), "Centro".into()], 0.1);
    let strings: hashbrown::HashMap<_, _> = messages.iter()
        .flat_map(|m| omsi_net::vars::decode(m, omsi_net::PROTOCOL as u8).unwrap().strings)
        .collect();
    assert!(strings.get(&0).unwrap().len() < fleet.len());
    apply_remote_strings(&mut vehicle, &strings);
    for name in ["ident", "number"] {
        let id = vehicle.ty.program.str_var(name).unwrap();
        assert_eq!(vehicle.state.str_vars[id as usize], fleet);
    }
    let display = vehicle.ty.program.str_var("IBIS_Display").unwrap();
    assert_eq!(vehicle.state.str_vars[display as usize], "Centro");
}

#[test]
fn wide_variable_sync_carries_stop_state_and_fleet_strings_beside_pose_state() {
    let mut program = omsi_script::Program::default();
    program.declare_var("Velocity");
    let stop = program.declare_script_var("haltewunsch");
    let ibis = program.declare_script_var("IBIS_mode");
    let permission = program.declare_script_var("door_handsteuerung");
    let plate = program.declare_str_var("ident");
    let number = program.declare_str_var("number");
    let table = var_table(&program);
    assert!(table.floats.contains(&(stop as u16)));
    assert!(table.floats.contains(&(ibis as u16)));
    // This script-owned door_ condition travels in our existing SyncTable instead.
    assert!(!table.floats.contains(&(permission as u16)));
    let mut tx = omsi_net::vars::VarSender::default();
    let messages = tx.tick(omsi_net::PROTOCOL as u8, 2, table.hash,
        &table.floats, &[1.0, 3.0], &table.strings,
        &["RZR0D16".into(), "285".into()], 0.1);
    let received: Vec<_> = messages.iter().map(|m| {
        assert!(m.len() <= omsi_net::MAX_DATAGRAM);
        omsi_net::vars::decode(m, omsi_net::PROTOCOL as u8).unwrap()
    }).collect();
    assert!(received.iter().all(|v| v.id == 2 && v.table == table.hash));
    let floats: Vec<_> = received.iter().flat_map(|v| v.floats.iter().copied()).collect();
    let strings: Vec<_> = received.iter().flat_map(|v| v.strings.iter().cloned()).collect();
    assert!(floats.contains(&(stop as u16, 1.0)));
    assert!(floats.contains(&(ibis as u16, 3.0)));
    assert!(strings.contains(&(plate as u16, "RZR0D16".into())));
    assert!(strings.contains(&(number as u16, "285".into())));
}

struct PaintFixture(PathBuf);
impl PaintFixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("openomsi-lan-paints-{}-{}",
            std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn scheme(&self, folder: &str, file: &str, contents: &[u8]) -> omsi_sim::vehicle::PaintScheme {
        let dir = self.0.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), contents).unwrap();
        omsi_sim::vehicle::PaintScheme {
            name: folder.to_string(), dir,
            textures: vec![("body".to_string(), file.to_string())],
            set_vars: vec![],
        }
    }
}
impl Drop for PaintFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[test]
fn interpolated_states_keep_current_fleet_metadata_and_allow_it_to_be_cleared() {
    let mut info = Pose {
        flags: omsi_net::FLAG_VEHICLE, sent_ms: 1234,
        bus: "Vehicles/test.bus".into(), number: "285".into(), ident: "RZR0D16".into(),
        bus_identity: "0123456789ABCDEF".into(), paint_identity: "FEDCBA9876543210".into(),
        ..Default::default()
    };
    let data = omsi_net::wire::encode_state(&info, omsi_net::PROTOCOL as u8, 1);
    let (_, _, mut sample) = omsi_net::wire::decode_state(&data, omsi_net::PROTOCOL as u8).unwrap();
    assert!(sample.number.is_empty() && sample.ident.is_empty());
    current_remote_info(&mut sample, &info);
    assert_eq!((&sample.number, &sample.ident), (&info.number, &info.ident));
    assert_eq!(sample.bus_identity, info.bus_identity);
    info.number = "286".into();
    info.ident.clear();
    current_remote_info(&mut sample, &info);
    assert_eq!(sample.number, "286");
    assert!(sample.ident.is_empty());
    assert_eq!(sample.sent_ms, 1234);
}

#[test]
fn repaint_identity_uses_image_content_and_rejects_ambiguous_or_missing_images() {
    let fixture = PaintFixture::new();
    let a = fixture.scheme("a", "body.dds", b"paint A");
    let b = fixture.scheme("b", "body.dds", b"paint B");
    let renamed = fixture.scheme("renamed", "renamed.dds", b"paint A");
    let mut cache = PaintIdentities::default();
    let aid = paint_scheme_identity(&a, &[], &mut cache);
    let bid = paint_scheme_identity(&b, &[], &mut cache);
    let rid = paint_scheme_identity(&renamed, &[], &mut cache);
    assert!(!aid.is_empty());
    assert_ne!(aid, bid);
    assert_eq!(aid, rid);
    assert_eq!(unique_paint_match([bid.clone(), aid.clone()], &aid.to_lowercase()), Some(1));
    assert_eq!(unique_paint_match([aid.clone(), rid], &aid), None);
    let mut missing = a.clone();
    missing.textures[0].1 = "missing.dds".into();
    assert!(paint_scheme_identity(&missing, &[], &mut cache).is_empty());
    assert_eq!(unique_paint_match([String::new()], ""), None);
    let mut changed_vars = a.clone();
    changed_vars.set_vars.push(("window".into(), 1.0));
    assert_ne!(paint_scheme_identity(&changed_vars, &[], &mut cache), aid);
}

#[test]
fn runtime_repaint_name_and_identity_follow_the_same_selection() {
    let fixture = PaintFixture::new();
    let schemes = vec![fixture.scheme("Startup", "body.dds", b"startup"),
        fixture.scheme("Runtime", "body.dds", b"runtime")];
    let mut cache = PaintIdentities::default();
    for (selection, expected) in [
        (None, "Startup"),
        (Some(Some(1)), "Runtime"),
        (Some(None), ""),
        (Some(Some(99)), ""),
    ] {
        let name = current_paint_name(&schemes, selection, "Startup");
        assert_eq!(name, expected);
        if expected.is_empty() {
            assert!(name.is_empty());
        } else {
            let scheme = schemes.iter().find(|s| s.name == expected).unwrap();
            assert!(!paint_scheme_identity(scheme, &[], &mut cache).is_empty());
        }
    }
    assert_ne!(
        paint_scheme_identity(&schemes[0], &[], &mut cache),
        paint_scheme_identity(&schemes[1], &[], &mut cache)
    );
}

/// Every session starts with every bus offered: a server joined before (on a phone the
/// launcher and the game share one process) no longer limits a drive alone or the next
/// server, and an answer late for an ended session is dropped (#1183).
#[test]
fn a_new_session_forgets_the_last_servers_buses() {
    let mut o = ServerOffers { session: 0, list: None };
    let first = o.reset();
    assert!(o.answer(first, &["Vehicles/MAN_SD200/MAN_SD77.bus".to_string()]));
    assert!(o.list.as_ref().is_some_and(|l| offers(l, "Vehicles/MAN_SD200/MAN_SD77.bus")));
    let second = o.reset();
    assert!(o.list.is_none());
    // (the first server's answer, late)
    assert!(!o.answer(first, &["Vehicles/MAN_SD200/MAN_SD77.bus".to_string()]));
    assert!(o.list.is_none());
    assert!(o.answer(second, &["Vehicles/MAN_SD202/MAN_D92.bus".to_string()]));
    assert!(o.list.as_ref().is_some_and(|l| !offers(l, "Vehicles/MAN_SD200/MAN_SD77.bus") && offers(l, "Vehicles/MAN_SD202/MAN_D92.bus")));
}

/// Whom a joining game asks for the buses offered: a server's web address, or a host's
/// address over UDP (its status page is found from there); nobody for a code or a search
/// (#1183).
#[test]
fn the_buses_offered_are_asked_at_an_address() {
    assert_eq!(offers_query_target("https://abc.trycloudflare.com").as_deref(), Some("https://abc.trycloudflare.com"));
    assert_eq!(offers_query_target(" 203.0.113.5:27015 ").as_deref(), Some("203.0.113.5:27015"));
    assert_eq!(offers_query_target("bus.example.org").as_deref(), Some("bus.example.org"));
    assert_eq!(offers_query_target("27015").as_deref(), Some("127.0.0.1:27015"));
    assert_eq!(offers_query_target("auto"), None);
    assert_eq!(offers_query_target(""), None);
    let code = omsi_net::SessionCode { session: 0x1234_5678_9abc, port: 27015, ips: vec!["192.168.1.20".parse().unwrap()], protocol: omsi_net::PROTOCOL as u8 }.encode();
    assert_eq!(offers_query_target(&code), None);
}

/// A server's `vehicles` list (as its `server.cfg` writes it, or every bus under its own
/// content folder) against the game's lists' `Vehicles/<folder>/<file>`: the same file
/// only (#1183).
#[test]
fn a_server_offers_the_buses_of_its_list_only() {
    let list = offered_keys(&["Vehicles\\MAN_SD200\\MAN_SD77.bus".to_string(), "OMSI 2/Vehicles/MAN_SD202/MAN_D92.bus".to_string()]);
    assert!(offers(&list, "Vehicles/MAN_SD200/MAN_SD77.bus"));
    assert!(offers(&list, "vehicles/man_sd202/man_d92.bus"));
    assert!(offers(&list, "Archives/pack.zip/Vehicles/MAN_SD202/MAN_D92.bus"));
    assert!(!offers(&list, "Vehicles/MAN_SD200/MAN_SD83.bus"));
    assert!(!offers(&list, "Vehicles/MAN_SD202/MAN_D86.bus"));
    // (the same file name in another folder is another bus)
    assert!(!offers(&list, "Vehicles/Mod_SD200/MAN_SD77.bus"));
    assert!(!offers(&offered_keys(&[]), "Vehicles/MAN_SD200/MAN_SD77.bus"));
}

/// The maps show another player with their bus, on foot where they walk, riding in a
/// third player's bus with that bus, and not at all riding in ours (#1011, #1080).
#[test]
fn the_maps_show_a_player_where_they_are() {
    let bus = (DVec3::new(100.0, 200.0, 5.0), 90.0);
    let third = |id: u32| (id == 7).then_some((DVec3::new(-50.0, 10.0, 0.0), 180.0));
    let mut pose = Pose { id: 3, ..Default::default() };
    let p = nav_player(&pose, " Anna ", bus, 2, third).unwrap();
    assert_eq!((p.position, p.heading, p.name.as_str()), (bus.0, 90.0, "Anna"));
    pose.walker = Some(omsi_net::Walker { x: 1.0, y: 2.0, z: 3.0, heading: 45.0, ..Default::default() });
    let p = nav_player(&pose, "", bus, 2, third).unwrap();
    assert_eq!((p.position, p.heading, p.name.as_str()), (DVec3::new(1.0, 2.0, 3.0), 45.0, "player 3"));
    pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard { owner: 7, ..Default::default() });
    assert_eq!(nav_player(&pose, "Anna", bus, 2, third).unwrap().position, DVec3::new(-50.0, 10.0, 0.0));
    pose.walker.as_mut().unwrap().aboard = Some(omsi_net::Aboard { owner: 2, ..Default::default() });
    assert!(nav_player(&pose, "Anna", bus, 2, third).is_none());
}


/// What is seen comes before what is heard in the capped values list: the AA-FR Agora
/// L's sound variables filled it in name order before its roller blind's scroll.
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn the_roller_blind_scroll_is_in_the_sync_table() {
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_4d_main.bus");
    require_content(&[&bus]);
    let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
    let t = SyncTable::new(&ty, &[]);
    assert!(t.values.len() <= omsi_net::wire::MAX_VALUES);
    for want in ["Rollband_Linie_Trans", "Rollband_Linie_Trans_2"] {
        assert!(t.values.iter().any(|v| v.0.eq_ignore_ascii_case(want)), "no {want}: {}", t.describe());
    }
}

/// An articulated bus's rear section is in its sync table: its lamps, displays' switches
/// and outside sounds (the AA-FR Agora L's rear section stood dark and silent in the
/// other players' games).
#[test]
#[ignore = "needs OMSI_ROOT with AA-FR_BusBundle (not part of the stock install)"]
fn the_rear_section_is_in_the_sync_table() {
    let root = original_root();
    let bus = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_main.bus");
    let trail = root.join("Vehicles/AA-FR_BusBundle/2002_Agora_L_3d_trail.bus");
    require_content(&[&bus, &trail]);
    let ty = omsi_sim::VehicleType::load(&root, &bus).expect("Agora L");
    let part = Arc::new(omsi_sim::VehicleType::load(&root, &trail).expect("Agora L trail"));
    let alone = SyncTable::new(&ty, &[]);
    let whole = SyncTable::new(&ty, &[part]);
    let count = |t: &SyncTable| t.lamps.len() + t.switches.len();
    assert!(count(&whole) > count(&alone), "alone {}, whole {}", alone.describe(), whole.describe());
    assert!(!whole.part_sounds.is_empty(), "no sounds for the rear section: {}", whole.describe());
    assert_ne!(whole.hash, alone.hash);
}

#[test]
fn day_numbers_and_clock_gaps() {
    assert_eq!(day_number(1990, 1) - day_number(1989, 365), 1);
    assert_eq!(day_number(1989, 1) - day_number(1988, 366), 1);
    let a = omsi_sim::SimClock {
        year: 1990,
        day_of_year: 1,
        time: 10.0,
        ..Default::default()
    };
    let b = omsi_sim::SimClock {
        year: 1989,
        day_of_year: 365,
        time: 86390.0,
        ..Default::default()
    };
    assert!((clock_gap(&a, &b) - 20.0).abs() < 1e-9);
    assert!((clock_gap(&b, &a) + 20.0).abs() < 1e-9);
    assert_eq!(parse_date("1989-05-30"), Some((1989, 150)));
    assert_eq!(parse_date("x"), None);
}

#[test]
fn the_hosts_clock_moves_on_past_midnight() {
    let h = omsi_net::HostClock {
        world: omsi_net::WorldInfo {
            date: "1989-12-31".into(),
            time: 86399.5,
            ..Default::default()
        },
        at: Instant::now() - Duration::from_secs(2),
        speed: 1.0,
    };
    let c = host_clock_now(&h).unwrap();
    assert_eq!((c.year, c.day_of_year), (1990, 1));
    assert!((c.time - 1.5).abs() < 0.1, "{}", c.time);
}

#[test]
fn engine_fed_names() {
    for n in [
        "Wheel_RotationSpeed_1_R",
        "Velocity",
        "AI_Light",
        "door_0",
        "StreetCond",
        "Timegap",
    ] {
        assert!(engine_fed(n), "{n}");
    }
    for n in [
        "engine_n",
        "engine_throttle_injection",
        "M_Wheel",
        "doorSpeed_0",
        "wiperpos",
        "cockpit_hupe_volume",
        "lights_stand",
    ] {
        assert!(!engine_fed(n), "{n}");
    }
}

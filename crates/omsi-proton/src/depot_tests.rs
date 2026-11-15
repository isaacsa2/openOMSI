use crate::{
    depot, import,
    tests::{Fixture, setup},
};
use omsi_vehicle::Hof;

#[test]
fn native_hof_keeps_text_destinations_reserved_flags_and_terminal_stop_identity() {
    let f = Fixture::new();
    setup(&f);
    for name in ["Start", "Garagem", "VIVA CIDADE"] {
        std::fs::create_dir_all(f.0.join("Town/dest").join(name)).unwrap();
    }
    std::fs::write(f.0.join("Town/dest/Garagem/OUTOFSERVICE.TXT"), "").unwrap();
    let stops = f.0.join("Town/tiles/0x0/busstops");
    std::fs::create_dir_all(&stops).unwrap();
    std::fs::write(stops.join("terminal.txt"), "[busstop]\nname=Terminal São João\nisLatestStop=1\nposX=300\nposY=7\nposZ=10\npaxAmount=0\n").unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    let hof = Hof::load(&f.0.join(depot::FILE)).unwrap();
    assert_eq!(report.destinations_imported, 4);
    assert_eq!(hof.name, "Town (Proton Bus)");
    assert_eq!(depot::installed_name(&f.0), Some(hof.name.clone()));
    // the services by name, then the stop that only ends a line
    assert_eq!(
        hof.termini
            .iter()
            .map(|t| t.menu_name())
            .collect::<Vec<_>>(),
        ["Garagem", "Start", "VIVA CIDADE", "Terminal São João"]
    );
    assert_eq!(
        hof.termini.iter().map(|t| t.code).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert!(hof.termini[0].all_exit);
    assert!(hof.termini[0].terminus_stop.is_none());
    assert!(hof.termini[1..].iter().all(|t| !t.all_exit));
    assert_eq!(
        hof.termini[3].terminus_stop.as_deref(),
        Some("Terminal São João")
    );
    assert_eq!(hof.termini[3].strings.len(), 6);
    assert_eq!(hof.termini[3].strings[0], "Terminal São João");
    assert!(
        hof.termini[3].strings[4].is_empty(),
        "no invented sign bitmap"
    );
    assert_eq!(hof.bus_stops[0].ident, "Terminal São João");
    assert!(hof.info_trips.is_empty());
    assert!(hof.info_busstop_lists.is_empty());
}

#[test]
fn tabbed_source_names_do_not_inject_native_hof_columns_or_records() {
    let f = Fixture::new();
    setup(&f);
    std::fs::write(
        f.0.join("Town/tiles/0x0/entrypoints.txt"),
        "[entrypoint_1]\nname=Rota\t42\t{ALLEX}\nposX=299.5\nposY=7\nposZ=10.5\n",
    )
    .unwrap();
    import(&f.0, "maps/Proton-Town", || false).unwrap();
    let hof = Hof::load(&f.0.join(depot::FILE)).unwrap();
    assert_eq!(hof.termini.len(), 1);
    assert_eq!(hof.termini[0].menu_name(), "Rota 42 {ALLEX}");
    assert!(!hof.termini[0].all_exit);
}

#[test]
fn map_depot_hint_is_bounded_and_corrupt_hints_are_ignored() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.0.join("proton-import")).unwrap();
    for bad in [
        "x".repeat(1025),
        "Depot\n[addterminus]".into(),
        String::new(),
    ] {
        std::fs::write(f.0.join(depot::NAME_FILE), bad).unwrap();
        assert!(depot::installed_name(&f.0).is_none());
    }
    assert_eq!(
        depot::name("maps/Proton-Town"),
        depot::name("maps/proton-town")
    );
    assert_ne!(
        depot::name("maps/Proton-Town"),
        depot::name("maps/Proton-Town2")
    );
}

fn imported_hof(services: &[&str], stops: &[&str]) -> Hof {
    let f = Fixture::new();
    setup(&f);
    for name in services {
        std::fs::create_dir_all(f.0.join("Town/dest").join(name)).unwrap();
    }
    let dir = f.0.join("Town/tiles/0x0/busstops");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, name) in stops.iter().enumerate() {
        std::fs::write(
            dir.join(format!("stop{i}.txt")),
            format!(
                "[busstop]\nname={name}\nisLatestStop=1\nposX={}\nposY=7\nposZ=10\npaxAmount=0\n",
                299.6 + i as f64 * 0.2
            ),
        )
        .unwrap();
    }
    import(&f.0, "maps/Proton-Town", || false).unwrap();
    Hof::load(&f.0.join(depot::FILE)).unwrap()
}

#[test]
fn lines_come_first_in_the_order_a_driver_reads_them_and_stops_never_move_their_codes() {
    let services = ["490 TERMINAL PIMENTAS", "44 METACIL", "420AT TREVO", "Garagem", "10 Rota", "2 Rota"];
    let few = imported_hof(&services, &["Zeta", "Alfa"]);
    let names: Vec<_> = few.termini.iter().map(|t| t.menu_name()).collect();
    assert_eq!(
        names,
        ["2 Rota", "10 Rota", "44 METACIL", "420AT TREVO", "490 TERMINAL PIMENTAS", "Garagem", "Start", "Alfa", "Zeta"]
    );
    assert_eq!(
        few.termini.iter().map(|t| t.code).collect::<Vec<_>>(),
        (1..=9).collect::<Vec<_>>()
    );
    // another stop that ends a line: every service keeps its code
    let more = imported_hof(&services, &["Zeta", "Alfa", "Meio"]);
    assert_eq!(more.termini.len(), few.termini.len() + 1);
    for (a, b) in few.termini[..7].iter().zip(&more.termini[..7]) {
        assert_eq!((a.menu_name(), a.code), (b.menu_name(), b.code));
    }
    assert_eq!(more.termini[8].menu_name(), "Meio");
}

#[test]
fn a_dest_folder_that_differs_only_in_case_is_the_same_destination() {
    let f = Fixture::new();
    setup(&f);
    std::fs::write(
        f.0.join("Town/tiles/0x0/entrypoints.txt"),
        "[entrypoint_1]\nname=821 CARMELA (parcial)\nposX=299.5\nposY=7\nposZ=10.5\nrotY=90\n",
    )
    .unwrap();
    std::fs::create_dir_all(f.0.join("Town/dest/821 CARMELA (Parcial)")).unwrap();
    std::fs::write(f.0.join("Town/dest/821 CARMELA (Parcial)/outofservice.txt"), "").unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    let hof = Hof::load(&f.0.join(depot::FILE)).unwrap();
    assert_eq!(report.destinations_imported, 1);
    assert_eq!(hof.termini.len(), 1);
    assert_eq!(hof.termini[0].menu_name(), "821 CARMELA (parcial)", "the entry point's spelling");
    assert!(hof.termini[0].all_exit, "the folder's out-of-service mark still counts");
    assert!(
        report.warnings.iter().any(|w| w.contains("differs only in case")),
        "{:?}",
        report.warnings
    );
}

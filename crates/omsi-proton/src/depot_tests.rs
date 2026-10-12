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
    assert_eq!(
        hof.termini
            .iter()
            .map(|t| t.menu_name())
            .collect::<Vec<_>>(),
        ["Garagem", "Start", "Terminal São João", "VIVA CIDADE"]
    );
    assert_eq!(
        hof.termini.iter().map(|t| t.code).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert!(hof.termini[0].all_exit);
    assert!(hof.termini[0].terminus_stop.is_none());
    assert!(hof.termini[1..].iter().all(|t| !t.all_exit));
    assert_eq!(
        hof.termini[2].terminus_stop.as_deref(),
        Some("Terminal São João")
    );
    assert_eq!(hof.termini[2].strings.len(), 6);
    assert_eq!(hof.termini[2].strings[0], "Terminal São João");
    assert!(
        hof.termini[2].strings[4].is_empty(),
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

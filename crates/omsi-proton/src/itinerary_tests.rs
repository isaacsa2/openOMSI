use super::*;
use crate::{
    gameplay_tests::{config, loaded, sco},
    import,
    tests::{setup, Fixture},
};

fn known() -> Vec<Known> {
    [("centro", "Praca Central"), ("sul_a", "Matriz"), ("sul_b", "Matriz"), ("fim", "Terminal Pimentas")]
        .iter()
        .map(|(id, name)| Known { id: id.to_string(), name: name.to_string() })
        .collect()
}

fn destinations() -> BTreeMap<String, bool> {
    [("490 TERMINAL PIMENTAS", false), ("Terminal Pimentas", false), ("Garagem", true), ("Start", false)]
        .iter()
        .map(|(n, r)| (n.to_string(), *r))
        .collect()
}

fn raw(destination: &str, stops: &[&str]) -> Raw {
    Raw {
        at: 1,
        destination: Some(destination.into()),
        stops: stops.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn why(raw: &Raw) -> String {
    match resolve(raw, &known(), &destinations()) {
        Ok(_) => panic!("{raw:?} should not resolve"),
        Err(why) => why,
    }
}

#[test]
fn the_file_is_read_with_sections_keys_comments_and_any_line_ending() {
    let text = "\u{feff}; a note\r\n# another\r\n[Itinerary]\r\nLine = 490\r\ndirection=Centro\r\ndestination=Start\r\nstop=Praca Central\r\nstop=@fim\r\n\r\n[itinerary]\r\ndestination=Garagem\r\nstop=Matriz\r\nstop=Praca Central\r\n";
    let parsed = parse(text.as_bytes()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].line.as_deref(), Some("490"));
    assert_eq!(parsed[0].direction.as_deref(), Some("Centro"));
    assert_eq!(parsed[0].stops, ["Praca Central", "@fim"]);
    assert_eq!((parsed[0].at, parsed[1].at), (3, 10));
    assert_eq!(parsed[1].line, None);
    // Windows-1252 text from an old editor reads as the author wrote it.
    let latin = b"[itinerary]\ndestination=Start\nstop=Pra\xe7a Central\nstop=Matriz\n";
    assert_eq!(parse(latin).unwrap()[0].stops[0], "Pra\u{e7}a Central");
    assert!(parse(b"; nothing here\n").unwrap().is_empty());
}

#[test]
fn nothing_irregular_is_guessed_at() {
    let bad = |text: &str| parse(text.as_bytes()).unwrap_err().to_string();
    assert!(bad("line=490\n").contains("line 1") && bad("line=490\n").contains("outside"));
    assert!(bad("[route]\n").contains("unknown section"));
    assert!(bad("[itinerary]\nlinha=1\n").contains("unknown key linha"));
    assert!(bad("[itinerary]\nstop\n").contains("key=value"));
    assert!(bad("[itinerary]\nline=1\nline=2\n").contains("given twice"));
    assert!(bad("[itinerary]\ndestination=A\ndestination=B\n").contains("line 3"));
    assert!(bad("[itinerary]\nstop=\n").contains("empty"));
    assert!(bad("[itinerary]\nstop=a\u{0007}b\n").contains("control"));
    assert!(bad(&format!("[itinerary]\nstop={}\n", "x".repeat(MAX_TEXT + 1))).contains("too long"));
    let many_stops = format!("[itinerary]\n{}", "stop=a\n".repeat(MAX_STOPS + 1));
    assert!(bad(&many_stops).contains("more than"));
    let many = "[itinerary]\n".repeat(MAX_ITINERARIES + 1);
    assert!(bad(&many).contains("more than"));
    let total = ("[itinerary]\n".to_string() + &"stop=a\n".repeat(MAX_STOPS)).repeat(MAX_REFERENCES / MAX_STOPS + 1);
    assert!(bad(&total).contains("too many stops in all"));
    // the stops of one itinerary may repeat (a loop line), only the key may not
    assert_eq!(parse(b"[itinerary]\nstop=a\nstop=b\nstop=a\n").unwrap()[0].stops.len(), 3);
}

#[test]
fn a_stop_is_named_by_one_exact_name_or_by_its_file() {
    let k = known();
    assert_eq!(resolve_stop(&k, "praca central").unwrap().id, "centro");
    assert_eq!(resolve_stop(&k, "@SUL_B").unwrap().id, "sul_b");
    // nothing is matched by a prefix, a likeness or a number inside a longer name
    assert!(resolve_stop(&k, "Praca").is_err());
    assert!(resolve_stop(&k, "Terminal").is_err());
    assert!(resolve_stop(&k, "@sul").is_err());
    let ambiguous = resolve_stop(&k, "Matriz").unwrap_err();
    assert!(ambiguous.contains("2 stops") && ambiguous.contains("@sul_a") && ambiguous.contains("@sul_b"), "{ambiguous}");
}

#[test]
fn an_itinerary_resolves_to_the_stops_it_names_in_order() {
    let mut r = raw("490 terminal pimentas", &["Praca Central", "@sul_b", "Terminal Pimentas"]);
    r.line = Some("490".into());
    r.direction = Some("Centro".into());
    let d = resolve(&r, &known(), &destinations()).unwrap();
    assert_eq!(d.destination, "490 TERMINAL PIMENTAS");
    assert_eq!(d.line.as_deref(), Some("490"));
    assert_eq!(d.stops.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["centro", "sul_b", "fim"]);
    // a loop may call at a stop twice, but not twice in a row
    let looped = resolve(&raw("Start", &["Praca Central", "@sul_a", "Praca Central"]), &known(), &destinations()).unwrap();
    assert_eq!(looped.stops.len(), 3);
}

#[test]
fn what_cannot_be_told_for_certain_is_left_out_with_its_reason() {
    assert!(why(&raw("Start", &["Praca Central"])).contains("at least two"));
    assert!(why(&raw("Start", &["Praca Central", "Inventada"])).contains("no imported stop"));
    assert!(why(&raw("Start", &["Praca Central", "Matriz"])).contains("@sul_a or @sul_b"));
    assert!(why(&raw("Start", &["Praca Central", "@centro"])).contains("twice in a row"));
    assert!(why(&raw("Outro", &["Praca Central", "@fim"])).contains("no destination of the generated HOF"));
    assert!(why(&raw("Garagem", &["Praca Central", "@fim"])).contains("out-of-service"));
    // 490 is not 4900: only the whole destination counts
    assert!(why(&raw("490", &["Praca Central", "@fim"])).contains("no destination"));
    assert!(why(&Raw { at: 1, stops: vec!["a".into(), "b".into()], ..Default::default() }).contains("no destination"));
    // a destination that is a stop's name is the terminal, so it must come last
    let wrong = why(&raw("Terminal Pimentas", &["@fim", "Praca Central"]));
    assert!(wrong.contains("is a stop, but the last stop is"), "{wrong}");
    assert!(resolve(&raw("Terminal Pimentas", &["Praca Central", "@fim"]), &known(), &destinations()).is_ok());
}

#[test]
fn the_same_service_twice_is_one_and_two_that_disagree_are_neither() {
    let mut a = raw("Start", &["Praca Central", "@sul_a"]);
    a.line = Some("1".into());
    let mut b = raw("Start", &["Praca Central", "@sul_a"]);
    b.line = Some("1".into());
    b.at = 5;
    let mut report = Report::default();
    let kept = drafts(&[a, b], &known(), &destinations(), &mut report);
    assert_eq!(kept.len(), 1);
    assert!(report.warnings[0].contains("line 5") && report.warnings[0].contains("first is kept"));

    let mut a = raw("Start", &["Praca Central", "@sul_a"]);
    a.line = Some("1".into());
    let mut c = raw("Start", &["Praca Central", "@sul_b"]);
    c.line = Some("1".into());
    c.at = 7;
    let mut d = raw("Start", &["Praca Central", "@sul_b"]);
    d.line = Some("2".into());
    d.at = 9;
    let mut report = Report::default();
    let kept = drafts(&[a, c, d], &known(), &destinations(), &mut report);
    assert_eq!(kept.len(), 1, "only the line that is not in doubt stays");
    assert_eq!(kept[0].line.as_deref(), Some("2"));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("tell them apart")).count(), 2);
}

#[test]
fn the_reader_refuses_what_the_importer_never_writes() {
    let ok = br#"{"version":1,"itineraries":[{"line":null,"direction":null,"destination":"Start","stops":[{"object":1000000,"id":"a","name":"A"},{"object":1000002,"id":"b","name":"B"}]}]}"#;
    assert_eq!(from_bytes(ok).unwrap()[0].stops[1].object, 1_000_002);
    let one_stop = String::from_utf8_lossy(ok).replace(r#",{"object":1000002,"id":"b","name":"B"}"#, "");
    assert!(from_bytes(one_stop.as_bytes()).is_err());
    assert!(from_bytes(String::from_utf8_lossy(ok).replace(r#""version":1"#, r#""version":2"#).as_bytes()).is_err());
    assert!(from_bytes(b"{}").is_err());
    assert!(from_bytes(&vec![b' '; (8 << 20) + 1]).is_err());
}

fn stop_files(f: &Fixture) {
    config(f, "busstops", "centro.txt", "[busstop]\nname=Praca Central\nposX=301\nposZ=10\nposY=7\npaxAmount=0\n");
    config(f, "busstops", "sul_a.txt", "[busstop]\nname=Matriz\nposX=305\nposZ=10\nposY=7\npaxAmount=0\n");
    config(f, "busstops", "sul_b.txt", "[busstop]\nname=Matriz\nposX=309\nposZ=10\nposY=7\npaxAmount=0\n");
    config(f, "busstops", "fim.txt", "[busstop]\nname=Terminal Pimentas\nisLatestStop=1\nposX=313\nposZ=10\nposY=7\npaxAmount=0\n");
    config(f, "busstops", "longe.txt", "[busstop]\nname=Longe\nposX=90000\nposZ=10\nposY=7\npaxAmount=0\n");
}

fn read_back(f: &Fixture) -> Vec<Itinerary> {
    from_bytes(&omsi_cfg::vfs::read(&f.0.join(SEALED)).unwrap()).unwrap()
}

#[test]
fn an_author_file_becomes_itineraries_that_name_the_native_stop_objects() {
    let f = Fixture::new();
    setup(&f);
    stop_files(&f);
    std::fs::write(
        f.0.join("Town.itineraries.txt"),
        "[itinerary]\nline=490\ndirection=Centro\ndestination=Terminal Pimentas\nstop=Praca Central\nstop=@sul_b\nstop=Terminal Pimentas\n\n\
         [itinerary]\nline=491\ndestination=Start\nstop=Praca Central\nstop=Matriz\n\n\
         [itinerary]\nline=492\ndestination=Start\nstop=Praca Central\nstop=Longe\n",
    )
    .unwrap();
    let original = std::fs::read(f.0.join("Town.map.txt")).unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.itineraries_imported, 1);
    assert_eq!(report.stops_imported, 4, "the stop beyond the scenery is not placed");
    assert!(report.warnings.iter().any(|w| w.contains("line 9") && w.contains("@sul_a or @sul_b")), "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.contains("\"Longe\" is not in the converted map")), "{:?}", report.warnings);
    assert_eq!(std::fs::read(f.0.join("Town.map.txt")).unwrap(), original, "the original configuration is not touched");

    let itineraries = read_back(&f);
    assert_eq!(itineraries.len(), 1);
    let it = &itineraries[0];
    assert_eq!((it.line.as_deref(), it.direction.as_deref()), (Some("490"), Some("Centro")));
    assert_eq!(it.destination, "Terminal Pimentas");
    // each reference is the object id of that very stop in the written tiles
    let tiles = loaded(&f);
    let objects: Vec<_> = tiles.iter().flat_map(|t| &t.objects).filter(|o| sco(&f, o).is_bus_stop).collect();
    assert_eq!(objects.len(), 4);
    for (stop, (id, name)) in it.stops.iter().zip([("centro", "Praca Central"), ("sul_b", "Matriz"), ("fim", "Terminal Pimentas")]) {
        assert_eq!((stop.id.as_str(), stop.name.as_str()), (id, name));
        let object = objects.iter().find(|o| o.id == stop.object as i64).expect("an object with that id");
        assert_eq!(object.extra[0], name);
    }
    // (x within the tile that starts at 300: the second stop is sul_b's, not sul_a's)
    let positions: Vec<f64> = it.stops.iter().map(|s| objects.iter().find(|o| o.id == s.object as i64).unwrap().pos[0]).collect();
    assert_eq!(positions, [1.0, 9.0, 13.0], "the two Matriz stops are told apart by their file");
}

#[test]
fn without_the_file_the_map_keeps_its_free_mode() {
    let f = Fixture::new();
    setup(&f);
    stop_files(&f);
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.itineraries_imported, 0);
    assert!(!f.0.join(SEALED).exists() && !f.0.join(FILE).exists());
    assert!(!report.warnings.iter().any(|w| w.contains("itinerar")));
}

#[test]
fn a_file_with_nothing_usable_leaves_no_output_and_a_broken_one_stops_the_import() {
    let f = Fixture::new();
    setup(&f);
    stop_files(&f);
    std::fs::write(f.0.join("town.ITINERARIES.txt"), "[itinerary]\ndestination=Start\nstop=Praca Central\n").unwrap();
    let report = import(&f.0, "maps/Proton-Town", || false).unwrap();
    assert_eq!(report.itineraries_imported, 0);
    assert!(report.warnings.iter().any(|w| w.contains("at least two")));
    assert!(!f.0.join(SEALED).exists());

    let g = Fixture::new();
    setup(&g);
    stop_files(&g);
    std::fs::write(g.0.join("Town.itineraries.txt"), "[itinerary]\ndestination=Start\ndestination=Start\n").unwrap();
    let error = format!("{:#}", import(&g.0, "maps/Proton-Town", || false).unwrap_err());
    assert!(error.contains("itineraries file, line 3"), "{error}");
    assert!(!g.0.join(SEALED).exists());
}

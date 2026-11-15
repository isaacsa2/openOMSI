use super::*;
use crate::{
    depot::{self, Destination},
    import,
    tests::{setup, Fixture},
};
use omsi_vehicle::Hof;

fn destinations() -> Destinations {
    [
        ("490 TERMINAL PIMENTAS", true),
        ("Garagem", true),
        ("Terminal Sao Joao", false),
        ("garagem", false),
    ]
    .iter()
    .map(|(n, service)| {
        (
            n.to_string(),
            Destination {
                reserved: false,
                service: *service,
            },
        )
    })
    .collect()
}

fn raw(name: &str, text: Option<&str>, via: Option<&str>) -> Raw {
    Raw {
        at: 1,
        name: Some(name.into()),
        sign: Sign {
            text: text.map(Into::into),
            via: via.map(Into::into),
            out_of_service: None,
        },
    }
}

#[test]
fn the_file_is_read_with_sections_keys_comments_and_any_line_ending() {
    let text = "\u{feff}; a note\r\n# another\r\n[Destination]\r\nName = 490 TERMINAL PIMENTAS\r\nTEXT=TERM. SAO JOAO\r\nvia=VIA LAVRAS\r\nout_of_service=0\r\n\r\n[destination]\r\nname=Garagem\r\nout_of_service=1\r\n";
    let parsed = parse(text.as_bytes()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name.as_deref(), Some("490 TERMINAL PIMENTAS"));
    assert_eq!(parsed[0].sign.text.as_deref(), Some("TERM. SAO JOAO"));
    assert_eq!(parsed[0].sign.via.as_deref(), Some("VIA LAVRAS"));
    assert_eq!(parsed[0].sign.out_of_service, Some(false));
    assert_eq!((parsed[0].at, parsed[1].at), (3, 9));
    assert_eq!(parsed[1].sign.out_of_service, Some(true));
    assert_eq!(parsed[1].sign.text, None);
    // text from an old editor reads as the author wrote it
    let latin = b"[destination]\nname=Garagem\ntext=S\xc3O JO\xc3O\n";
    assert_eq!(
        parse(latin).unwrap()[0].sign.text.as_deref(),
        Some("S\u{c3}O JO\u{c3}O")
    );
    let utf8 = "[destination]\nname=Garagem\ntext=S\u{c3}O JO\u{c3}O\n";
    assert_eq!(
        parse(utf8.as_bytes()).unwrap()[0].sign.text.as_deref(),
        Some("S\u{c3}O JO\u{c3}O")
    );
    assert!(parse(b"; nothing here\n").unwrap().is_empty());
}

#[test]
fn nothing_irregular_is_guessed_at() {
    let bad = |text: &str| parse(text.as_bytes()).unwrap_err().to_string();
    let outside = bad("text=x\n");
    assert!(
        outside.contains("line 1") && outside.contains("outside"),
        "{outside}"
    );
    assert!(
        bad("[destination]\nname=a\ncolour=red\n").contains("line 3")
            && bad("[destination]\nname=a\ncolour=red\n").contains("unknown key")
    );
    assert!(
        bad("[destination]\nname=a\n[stop]\n").contains("line 3")
            && bad("[destination]\nname=a\n[stop]\n").contains("unknown section")
    );
    assert!(
        bad("[destination]\nname=a\nname=b\n").contains("line 3")
            && bad("[destination]\nname=a\nname=b\n").contains("twice")
    );
    assert!(bad("[destination]\nname=a\ntext=1\ntext=2\n").contains("twice"));
    assert!(bad("[destination]\nname=a\nvia=\n").contains("via is empty"));
    assert!(bad("[destination]\nname=a\ntext=a\tb\n").contains("control character"));
    assert!(bad(&format!(
        "[destination]\nname=a\ntext={}\n",
        "x".repeat(257)
    ))
    .contains("too long"));
    assert!(bad("[destination]\nname=a\nout_of_service=yes\n").contains("0 or 1"));
    assert!(bad("[destination]\nname=a\nout_of_service=1\nout_of_service=1\n").contains("twice"));
    assert!(bad("[destination]\nname\n").contains("expected key=value"));
    let many = "[destination]\n".repeat(MAX_ENTRIES + 1);
    assert!(bad(&many).contains("more than"));
}

#[test]
fn a_name_resolves_exactly_or_by_case_and_a_mistake_is_left_out_and_reported() {
    let d = destinations();
    // exactly, even though another destination differs from it in case only
    assert_eq!(
        resolve(&raw("Garagem", Some("G"), None), &d).unwrap(),
        "Garagem"
    );
    assert_eq!(
        resolve(&raw("garagem", Some("G"), None), &d).unwrap(),
        "garagem"
    );
    // by case alone
    assert_eq!(
        resolve(&raw("490 terminal pimentas", Some("T"), None), &d).unwrap(),
        "490 TERMINAL PIMENTAS"
    );
    assert!(resolve(&raw("GARAGEM", Some("G"), None), &d)
        .unwrap_err()
        .contains("several"));
    assert!(
        resolve(&raw("490", Some("T"), None), &d)
            .unwrap_err()
            .contains("no destination"),
        "490 is not 4900"
    );
    assert!(resolve(&raw("Nada", Some("T"), None), &d)
        .unwrap_err()
        .contains("no destination"));
    assert!(resolve(&raw("Garagem", None, None), &d)
        .unwrap_err()
        .contains("gives no"));
    assert!(resolve(
        &Raw {
            at: 1,
            ..Default::default()
        },
        &d
    )
    .unwrap_err()
    .contains("no name"));
    let mut report = Report::default();
    let raws = [
        raw(
            "490 TERMINAL PIMENTAS",
            Some("TERM. SAO JOAO"),
            Some("VIA LAVRAS"),
        ),
        raw("Nada", Some("x"), None),
    ];
    let kept = signs(&raws, &d, &mut report);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        kept["490 TERMINAL PIMENTAS"].text.as_deref(),
        Some("TERM. SAO JOAO")
    );
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("line 1") && report.warnings[0].contains("left out"));
}

#[test]
fn the_same_destination_twice_is_one_and_two_that_disagree_are_neither() {
    let d = destinations();
    let mut again = raw("490 TERMINAL PIMENTAS", Some("A"), None);
    again.at = 5;
    let mut report = Report::default();
    let kept = signs(
        &[raw("490 TERMINAL PIMENTAS", Some("A"), None), again],
        &d,
        &mut report,
    );
    assert_eq!(kept["490 TERMINAL PIMENTAS"].text.as_deref(), Some("A"));
    assert_eq!(report.warnings.len(), 1);
    assert!(
        report.warnings[0].contains("line 5") && report.warnings[0].contains("the first is kept")
    );
    let mut other = raw("490 TERMINAL PIMENTAS", Some("B"), None);
    other.at = 7;
    let mut report = Report::default();
    let kept = signs(
        &[raw("490 TERMINAL PIMENTAS", Some("A"), None), other],
        &d,
        &mut report,
    );
    assert!(
        kept.is_empty(),
        "which of them the displays show would be a guess"
    );
    assert_eq!(report.warnings.len(), 2);
    assert!(report
        .warnings
        .iter()
        .all(|w| w.contains("declared again with other values")));
}

#[test]
fn out_of_service_marks_and_clears_the_reserved_flag() {
    let mut d = destinations();
    d.get_mut("490 TERMINAL PIMENTAS").unwrap().reserved = true;
    let mut signs = Signs::new();
    signs.insert(
        "Garagem".into(),
        Sign {
            out_of_service: Some(true),
            ..Default::default()
        },
    );
    signs.insert(
        "490 TERMINAL PIMENTAS".into(),
        Sign {
            out_of_service: Some(false),
            ..Default::default()
        },
    );
    signs.insert(
        "Terminal Sao Joao".into(),
        Sign {
            text: Some("x".into()),
            ..Default::default()
        },
    );
    apply(&signs, &mut d);
    assert!(d["Garagem"].reserved);
    assert!(
        !d["490 TERMINAL PIMENTAS"].reserved,
        "the author's 0 clears an outofservice.txt"
    );
    assert!(
        !d["Terminal Sao Joao"].reserved,
        "a text alone leaves the mark as it was"
    );
}

fn town(files: &[(&str, &str)], dest: &[&str]) -> (Fixture, Result<Report>) {
    let f = Fixture::new();
    setup(&f);
    for name in dest {
        std::fs::create_dir_all(f.0.join("Town/dest").join(name)).unwrap();
    }
    for (name, text) in files {
        std::fs::write(f.0.join(name), text).unwrap();
    }
    let report = import(&f.0, "maps/Proton-Town", || false);
    (f, report)
}

#[test]
fn the_hof_shows_the_authors_text_and_nothing_else_changes() {
    let (f, report) = town(
        &[(
            "Town.destinations.txt",
            "[destination]\nname=490 terminal pimentas\ntext=TERM. S\u{c3}O JO\u{c3}O\nvia=VIA LAVRAS\n\n[destination]\nname=Garagem\ntext=GARAGEM\nout_of_service=1\n\n[destination]\nname=Nao Existe\ntext=x\n",
        )],
        &["490 TERMINAL PIMENTAS", "Garagem", "494 LAVRAS"],
    );
    let report = report.unwrap();
    let hof = Hof::load(&f.0.join(depot::FILE)).unwrap();
    assert_eq!(report.destination_signs_imported, 2);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("line 11") && w.contains("Nao Existe")),
        "{:?}",
        report.warnings
    );
    let by = |name: &str| hof.termini.iter().find(|t| t.menu_name() == name).unwrap();
    let line = by("490 TERMINAL PIMENTAS");
    assert_eq!(
        line.display_name(),
        "TERM. S\u{c3}O JO\u{c3}O",
        "what the front display shows"
    );
    assert_eq!(line.strings.len(), 6);
    assert_eq!(line.strings[0], "TERM. S\u{c3}O JO\u{c3}O");
    assert_eq!(line.strings[1], "VIA LAVRAS", "the second line");
    assert_eq!(
        (line.strings[2].as_str(), line.strings[4].as_str()),
        ("", "")
    );
    assert_eq!(
        (line.strings[3].as_str(), line.strings[5].as_str()),
        (line.strings[0].as_str(), line.strings[0].as_str()),
        "side and rear signs show the first line"
    );
    assert!(!line.all_exit);
    let garage = by("Garagem");
    assert!(
        garage.all_exit && garage.terminus_stop.is_none(),
        "declared out of service"
    );
    assert_eq!(garage.display_name(), "GARAGEM");
    assert_eq!(
        garage.strings[1], "",
        "a text with no via has no second line"
    );
    // a destination the file does not mention keeps the layout it always had
    let plain = by("494 LAVRAS");
    assert_eq!(
        plain.strings,
        [
            "494 LAVRAS",
            "494 LAVRAS",
            "",
            "494 LAVRAS",
            "",
            "494 LAVRAS"
        ]
    );
}

#[test]
fn without_the_file_or_with_one_that_says_nothing_the_hof_is_the_same() {
    let dest = ["490 TERMINAL PIMENTAS", "Garagem"];
    let (a, report) = town(&[], &dest);
    report.unwrap();
    let (b, report) = town(&[("Town.destinations.txt", "; only a note\n")], &dest);
    let report = report.unwrap();
    assert_eq!(report.destination_signs_imported, 0);
    let (a, b) = (
        Hof::load(&a.0.join(depot::FILE)).unwrap(),
        Hof::load(&b.0.join(depot::FILE)).unwrap(),
    );
    assert_eq!(a.termini, b.termini);
    assert_eq!(a.bus_stops, b.bus_stops);
}

#[test]
fn a_broken_file_stops_the_import_and_names_the_line() {
    let (_f, report) = town(
        &[(
            "Town.destinations.txt",
            "[destination]\nname=Garagem\ncolour=red\n",
        )],
        &["Garagem"],
    );
    let why = format!("{:#}", report.unwrap_err());
    assert!(
        why.contains("destinations file, line 3") && why.contains("unknown key"),
        "{why}"
    );
}

#[test]
fn text_cannot_inject_columns_or_records_into_the_hof() {
    // a tab or a line break would start another column or record; neither can be written
    assert!(parse(b"[destination]\nname=Garagem\ntext=a\tb\n").is_err());
    let (f, report) = town(
        &[(
            "Town.destinations.txt",
            "[destination]\nname=Garagem\ntext=GARAGEM [end]\nvia={ALLEX}\n",
        )],
        &["Garagem"],
    );
    report.unwrap();
    let hof = Hof::load(&f.0.join(depot::FILE)).unwrap();
    let garage = hof
        .termini
        .iter()
        .find(|t| t.menu_name() == "Garagem")
        .unwrap();
    assert_eq!(
        (garage.strings[0].as_str(), garage.strings[1].as_str()),
        ("GARAGEM [end]", "{ALLEX}")
    );
    assert!(!garage.all_exit, "a text is never a flag");
    assert_eq!(hof.termini.len(), 2, "Garagem and Start");
}

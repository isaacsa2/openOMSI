use crate::tests::{setup, Fixture};
use std::path::Path;

fn names(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    out.sort();
    out
}

#[test]
fn denied_public_import_and_cli_never_create_output_or_edit_source() {
    for declaration in [
        "",
        "[openomsi]\nallowConversion=0",
        "[openomsi]\nallowConversion=",
        "[openomsi]\nallowConversion=1\nallowConversion=0",
    ] {
        let f = Fixture::new();
        setup(&f);
        let text = format!("[map]\nbaseDir=Town\nmodelsDir=0x0\nmapModVersion=4\n{declaration}\n");
        std::fs::write(f.0.join("Town.map.txt"), &text).unwrap();
        let before = names(&f.0);
        assert!(crate::import(&f.0, "maps/Proton-Town", || false)
            .unwrap_err()
            .to_string()
            .contains("Town.map.txt"));
        assert_eq!(names(&f.0), before);
        let output = f.0.with_extension("converted");
        assert!(crate::package::convert_folder(&f.0, &output, "Town").is_err());
        assert!(!output.exists());
        assert_eq!(
            std::fs::read_to_string(f.0.join("Town.map.txt")).unwrap(),
            text
        );
        assert!(!f.0.join("proton-import").exists());
    }
}

#[test]
fn source_preserving_package_loads_oi_links_and_keeps_attribution_bytes() {
    let parent = Fixture::new();
    let source = parent.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let f = Fixture(source.clone());
    setup(&f);
    let license = b"License\r\n\xff\x80exact original bytes";
    std::fs::write(source.join("LICENSE.cfg"), license).unwrap();
    std::fs::write(source.join("Town/Créditos.txt"), license).unwrap();
    let config = std::fs::read(source.join("Town.map.txt")).unwrap();
    let mesh = std::fs::read(source.join("Town/tiles/0x0/road.3ds")).unwrap();
    let output = parent.0.join("output");
    crate::package::convert_folder(&source, &output, "Town").unwrap();
    let map = output.join("maps/Proton-Town");
    assert_eq!(std::fs::read(map.join("LICENSE.cfg")).unwrap(), license);
    assert_eq!(
        std::fs::read(map.join("Town/Créditos.txt")).unwrap(),
        license
    );
    assert_eq!(std::fs::read(source.join("Town.map.txt")).unwrap(), config);
    assert_eq!(
        std::fs::read(source.join("Town/tiles/0x0/road.3ds")).unwrap(),
        mesh
    );
    assert!(!source.join("proton-import").exists());
    assert!(!map.join("global.cfg").exists());
    let global = omsi_map::GlobalCfg::load(&map.join(omsi_cfg::oi::GLOBAL)).unwrap();
    for tile in global.tiles {
        assert!(tile.file.ends_with(".tileoi"));
        for object in omsi_map::Tile::load(&map.join(tile.file)).unwrap().objects {
            assert!(object.file.ends_with(".oscoi"));
            assert!(output.join(object.file).is_file());
        }
    }
    assert_eq!(std::fs::read_dir(output.join("HOFs")).unwrap().count(), 1);
    assert!(crate::package::convert_folder(&source, &output, "Town").is_err());
}

#[test]
fn cli_failure_after_validation_discards_stage_and_keeps_source() {
    let parent = Fixture::new();
    let source = parent.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let f = Fixture(source.clone());
    setup(&f);
    std::fs::write(source.join("Town/tiles/0x0/road.3ds"), b"broken").unwrap();
    let output = parent.0.join("output");
    assert!(crate::package::convert_folder(&source, &output, "Town").is_err());
    assert!(!output.exists());
    assert_eq!(names(&parent.0), vec![source]);
    assert!(!f.0.join("proton-import").exists());
}

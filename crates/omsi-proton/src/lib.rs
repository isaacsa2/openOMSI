//! Declared conversion of user-supplied Proton Bus phase 2-4 maps. Conversion runs
//! in staging and emits OpenOMSI-only OI definitions for the native simulation.
//! Only free driving is supported. Source mods are never embedded in the program.
mod convert;
mod export;
pub mod depot;
pub mod permission;
pub mod package;
mod materials;
mod prefabs;
mod gameplay;
pub mod navigation;
pub mod traffic;
mod optimise;
mod paths;
mod settings;
mod signals;
mod stops;
mod texture_settings;
mod spawn;
mod spool;
mod tags;
mod streetlights;
pub mod three_ds;

use anyhow::{ensure, Context, Result};

use omsi_o3d::Material;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Ini = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Debug, Default)]
pub struct Report {
    pub tiles: usize,
    pub triangles: usize,
    pub markers_skipped: usize,
    pub gps_skipped: usize,
    pub gps_imported: usize,
    pub roads_imported: usize,
    pub lights_imported: usize,
    pub stops_imported: usize,
    pub destinations_imported: usize,
    pub prefabs_imported: usize,
    pub ai_vehicles_imported: usize,
    pub traffic_segments_imported: usize,
    pub pedestrian_segments_imported: usize,
    pub signals_imported: usize,
    pub signal_paths_imported: usize,
    pub vertices_removed: usize,
    pub warnings: Vec<String>,
}

fn ini(path: &Path) -> Result<Ini> {
    ensure!(std::fs::metadata(path)?.len() <= 1024 * 1024, "Proton configuration exceeds 1 MiB");
    let bytes = std::fs::read(path)?;
    Ok(ini_bytes(&bytes))
}

/// Read the authored asset folder without loading the map geometry.
pub fn base_dir(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.len() <= 64 * 1024,
        "Proton map configuration exceeds 64 KiB"
    );
    ini_bytes(bytes)
        .get("map")
        .and_then(|map| map.get("basedir"))
        .cloned()
        .context("missing Proton baseDir")
}

fn ini_bytes(bytes: &[u8]) -> Ini {
    let text = ini_text(bytes);
    let mut sections = Ini::new();
    let mut current = String::new();
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = s.to_ascii_lowercase();
        } else if let Some((k, v)) = line.split_once('=') {
            sections.entry(current.clone()).or_default().insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    sections
}

fn ini_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xff, 0xfe]) {
        return omsi_cfg::decode_text(bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1252
            .decode_without_bom_handling(&bytes)
            .0
            .into_owned(),
    }
}

/// Resolve only safe paths in the staging area, including a mod made on Windows whose
/// texture spelling differs in case. Never resolve through the game's other content roots.
fn child(base: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.replace('\\', "/");
    ensure!(
        !rel.is_empty() && !rel.starts_with('/') && !rel.contains([':', '\0']),
        "unsafe Proton path {rel:?}"
    );
    let mut out = base.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        ensure!(part != "..", "unsafe Proton path {rel:?}");
        let exact = out.join(part);
        if exact.exists() {
            out = exact;
        } else {
            out = std::fs::read_dir(&out)?
                .filter_map(|e| e.ok())
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))
                .map(|e| e.path())
                .unwrap_or(exact);
        }
        if let Ok(m) = std::fs::symlink_metadata(&out) {
            ensure!(!m.file_type().is_symlink(), "symbolic link in Proton map");
        }
    }
    Ok(out)
}

fn files(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        ensure!(!e.file_type()?.is_symlink(), "symbolic link in Proton map");
        if e.file_type()?.is_file()
            && e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(extension))
        {
            out.push(e.path());
        }
    }
    out.sort();
    Ok(out)
}

fn prefixes(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    // Vehicle paths use exact numbered marker names in paths::Collector.
    for folder in ["aipeople", "busstops", "trafficlights"] {
        let folder = child(dir, folder)?;
        if !folder.is_dir() {
            continue;
        }
        for f in files(&folder, "txt")? {
            let config = ini(&f)?;
            for section in config.values() {
                if let Some(p) = section.get("prefix").filter(|p| !p.is_empty()) {
                    out.push(p.clone());
                }
            }
        }
    }
    Ok(out)
}

pub(crate) struct Spawn {
    name: String,
    pos: [f64; 3], // x, forward, height
    heading: f64,
}

fn spawns(base: &Path, models: &Path, warnings: &mut Vec<String>) -> Result<Vec<Spawn>> {
    let mut out = Vec::new();
    for d in [base, models] {
        let p = child(d, "entrypoints.txt")?;
        if !p.is_file() {
            continue;
        }
        for (name, fields) in ini(&p)? {
            if !name.starts_with("entrypoint_") {
                continue;
            }
            let mut number = |key: &str| -> Result<f64> {
                spawn::number(fields.get(key), key, &name, &p, warnings)
            };
            let pos = [number("posx")?, number("posz")?, number("posy")?];
            let heading = number("roty")?;
            let spawn = Spawn {
                name: fields.get("name").cloned().unwrap_or(name),
                pos,
                heading,
            };
            if !out
                .iter()
                .any(|s: &Spawn| s.name == spawn.name && s.pos == spawn.pos)
            {
                out.push(spawn);
            }
        }
    }
    ensure!(
        !out.is_empty(),
        "Proton map has no explicit entry point for free driving"
    );
    Ok(out)
}

fn text(path: &Path, s: &str) -> Result<()> {
    let mut bytes = vec![0xff, 0xfe];
    for c in s.replace('\n', "\r\n").encode_utf16() {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn textures(
    source: &Path,
    dest: &Path,
    materials: &mut [Material],
    warnings: &mut Vec<String>,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for m in materials {
        ensure!(!cancel(), "map import cancelled");
        if m.texture.is_empty() {
            continue;
        }
        // 3DS exporters can write the artist's absolute path: only its basename is a mod asset.
        let name = m
            .texture
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_owned();
        let from = child(source, &name)?;
        if from.is_file() {
            let actual = from
                .file_name()
                .context("texture has no filename")?
                .to_string_lossy()
                .into_owned();
            if !dest.join(&actual).is_file() {
                std::fs::copy(&from, dest.join(&actual))?;
            }
            m.texture = actual;
        } else {
            warnings.push(format!("Missing texture: {name}"));
            m.texture.clear();
        }
    }
    Ok(())
}

/// `dir` is a fresh installer staging directory, not the source mod or installed map.
/// `installed_rel` is its eventual `maps/Proton-<name>` location below the content root.
pub fn import(dir: &Path, installed_rel: &str, cancel: impl Fn() -> bool) -> Result<Report> {
    ensure!(
        installed_rel.strip_prefix("maps/Proton-").is_some_and(|name| !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\n', '\r', '\0'])),
        "invalid imported map destination"
    );
    ensure!(
        !child(dir, "global.cfg")?.exists() && !child(dir, omsi_cfg::oi::GLOBAL)?.exists() && !child(dir, "proton-import")?.exists(),
        "source contains reserved import output files"
    );
    let configs: Vec<_> = files(dir, "txt")?
        .into_iter()
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                n.to_string_lossy()
                    .to_ascii_lowercase()
                    .ends_with(".map.txt")
            })
        })
        .collect();
    ensure!(configs.len() == 1, "expected one Proton .map.txt file");
    permission::validate_file(&configs[0]).with_context(|| format!("{}", configs[0].display()))?;
    let config = ini(&configs[0])?;
    let map = config.get("map").context("missing Proton [map] section")?;
    let version = map
        .get("mapmodversion")
        .context("missing mapModVersion")?
        .parse::<f32>()?;
    ensure!(
        version.is_finite() && (2.0..=4.0).contains(&version),
        "unsupported Proton map phase {version}"
    );
    let base = child(dir, map.get("basedir").context("missing baseDir")?)?;
    let models = child(
        &child(&base, "tiles")?,
        map.get("modelsdir").context("missing modelsDir")?,
    )?;
    let texture_dir = child(
        &base,
        map.get("textures")
            .map(String::as_str)
            .unwrap_or("textures"),
    )?;
    let mut report = Report::default();
    let mut spawns = spawns(&base, &models, &mut report.warnings)?;
    let markers = prefixes(&models)?;
    let mut spool = spool::Spool::new(&dir.join("proton-import/work"), &spawns)?;
    let mut registry = materials::Registry::default();
    let mut output_bytes = 0;
    let presets = texture_settings::Presets::read(&texture_dir, &mut report)?;
    let mut navigation = paths::Collector::new(&models, &spawns)?;
    let mut stops = stops::Collector::new(&models)?;
    let mut signals = signals::Collector::new(&models, &mut report)?;
    let mut lamps = streetlights::Collector::new(&models, &texture_dir, &dir.join("proton-import/texture"), &mut report)?;
    let mut helpers = 0;
    let mut embedded_trees = 0;
    let mut inputs: Vec<_> = files(&models, "3ds")?.into_iter().map(|model| prefabs::Placement { model, transform: glam::Mat4::IDENTITY }).collect();
    inputs.extend(prefabs::read(&base, &models, &mut report)?);
    for input in inputs {
        let f = input.model;
        ensure!(!cancel(), "map import cancelled");
        let mut scene = three_ds::Scene::open(&f, &cancel)
            .with_context(|| format!("reading {}", f.display()))?;
        if scene.trailing_bytes != 0 {
            report.warnings.push(format!(
                "{}: ignored {} trailing bytes outside the declared 3DS root chunk",
                f.file_name().unwrap_or_default().to_string_lossy(),
                scene.trailing_bytes
            ));
        }
        let all_gps = f
            .file_stem()
            .is_some_and(|s| optimise::gps_name(&s.to_string_lossy()));
        if scene.keyframes {
            report.warnings.push(
                "3DS keyframe data is present; only static mesh geometry is imported.".into(),
            );
        }
        let (names, default) = registry.register(&scene, &presets)?;
        scene
            .visit_import(&cancel, |mut object, repaired_uvs| {
                prefabs::transform(&mut object, input.transform)?;
                if repaired_uvs > 0 {
                    report.warnings.push(format!(
                        "{}: object {:?}: replaced {repaired_uvs} non-finite texture coordinates with (0, 0). Geometry and collision retained; affected texture mapping needs checking.",
                        f.file_name().unwrap_or_default().to_string_lossy(), object.name
                    ));
                }
                if all_gps || optimise::gps_name(&object.name) {
                    navigation.gps(&object, &f.file_name().unwrap_or_default().to_string_lossy(), &mut report)?;
                    return Ok(());
                }
                if lamps.marker(&object, &mut report)? {
                    return Ok(());
                }
                if navigation.marker(&object)? {
                    report.markers_skipped += 1;
                    return Ok(());
                }
                if stops.marker(&object)? || signals.marker(&object, &mut registry, &names, default, dir, &texture_dir, &mut report, &cancel)? {
                    report.markers_skipped += 1;
                    return Ok(());
                }
                if convert::marker(&object.name, &markers) {
                    report.markers_skipped += 1;
                    return Ok(());
                }
                let mode = match tags::classify(&object.name) {
                    tags::ObjectKind::Mesh(mode) => mode,
                    tags::ObjectKind::Helper => {
                        helpers += 1;
                        return Ok(());
                    }
                    tags::ObjectKind::EmbeddedTree => {
                        embedded_trees += 1;
                        return Ok(());
                    }
                };
                let faces = registry.object_faces(&object, &names, default, mode)?;
                convert::append_object(
                    &object,
                    &faces,
                    &registry.mappings,
                    &mut spool,
                    &mut output_bytes,
                    &cancel,
                )
                .with_context(|| format!("converting object {:?}", object.name))
            })
            .with_context(|| format!("reading {}", f.display()))?;
    }
    ensure!(
        !spool.tiles.is_empty(),
        "Proton map has no renderable 3DS geometry"
    );
    let backdrops = spool.finish_backdrops(&mut output_bytes, &cancel)?;
    if backdrops > 0 {
        report.warnings.push(format!("{backdrops} oversized flat backdrop faces were clipped to occupied scenery tiles; distant empty background was not imported."));
    }
    spool.finish_writes()?;
    if helpers > 0 {
        report.warnings.push(format!("{helpers} invisible gameplay/helper meshes were omitted; their simulation is not imported."));
    }
    if embedded_trees > 0 {
        report.warnings.push(format!("{embedded_trees} embedded tree placeholders were omitted; built-in Proton trees are not included."));
    }
    let (mut materials, modes) = registry.into_materials();
    for (i, m) in materials.iter_mut().enumerate() {
        if !spool.used.contains(&(i as u16)) {
            *m = Material::default();
        }
    }
    textures(
        &texture_dir,
        &dir.join("proton-import/texture"),
        &mut materials,
        &mut report.warnings,
        &cancel,
    )?;
    let keys: Vec<_> = spool.tiles.keys().copied().collect();
    for (i, &key) in keys.iter().enumerate() {
        ensure!(!cancel(), "map import cancelled");
        let tile = spool.mesh(key, &materials, &modes, &cancel)?;
        report.vertices_removed += tile.removed;
        spool.probe.nearest(key, &tile.visible);
        spool.probe.nearest(key, &tile.collision);
        let lights = lamps.tiles.remove(&key).unwrap_or_default();
        export::tile(dir, installed_rel, key, &tile, &lights, i)?;
        spool.discard_tile(key)?;
    }
    spool.probe.finish(&mut spawns, &mut report.warnings)?;
    ensure!(!cancel(), "map import cancelled");
    // Success must report a cleanup failure instead of silently ignoring it in Drop.
    spool.cleanup().context("removing temporary map geometry")?;
    if !lamps.tiles.is_empty() {
        let omitted: usize = lamps.tiles.values().map(Vec::len).sum();
        report.lights_imported -= omitted;
        report.warnings.push(format!("{omitted} streetlight markers outside scenery tiles were omitted"));
    }
    let (navigation, paths) = navigation.finish(&mut report)?;
    let mut gameplay = gameplay::Paths::new(&paths)?;
    drop(paths);
    let stops = stops.finish(&mut gameplay, &mut report)?;
    depot::export(dir, &base, installed_rel, &spawns, &stops, &mut report)?;
    gameplay::export(dir, installed_rel, &keys, gameplay, stops, signals, &mut report, &cancel)?;
    let file = std::fs::File::create(dir.join("proton-import/navigation.json"))?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &navigation)?;
    std::io::Write::flush(&mut writer)?;
    export::global(dir, installed_rel, &keys, &spawns)?;
    traffic::export(dir, &base, &texture_dir, installed_rel, &config, &mut report, &mut output_bytes, &cancel)?;
    package::seal(dir, installed_rel, &cancel)?;
    report.tiles = keys.len();
    report.triangles = spool.tiles.values().sum();
    report.warnings.sort();
    report.warnings.dedup();
    Ok(report)
}

#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod tag_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod gameplay_tests;
#[cfg(test)]
mod depot_tests;

#[cfg(test)]
mod package_tests;

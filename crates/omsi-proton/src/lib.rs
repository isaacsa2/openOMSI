//! Basic import of user-supplied Proton Bus phase 2-4 maps. Conversion runs in the
//! installer's staging area; the renderer continues to read ordinary OMSI scenery.
//! Only free driving is supported. Source mods are never embedded in the program.
mod convert;
mod optimise;
mod spawn;
pub mod three_ds;

use anyhow::{ensure, Context, Result};
use convert::{Tiles, EDGE};
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
    pub vertices_removed: usize,
    pub warnings: Vec<String>,
}

fn ini(path: &Path) -> Result<Ini> {
    let bytes = std::fs::read(path)?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1252
            .decode_without_bom_handling(&bytes)
            .0
            .into_owned(),
    };
    let mut sections = Ini::new();
    let mut current = String::new();
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = s.to_ascii_lowercase();
        } else if let Some((k, v)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    Ok(sections)
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
    for folder in ["aipeople", "aivehicles", "busstops", "trafficlights"] {
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

fn spawns(base: &Path, models: &Path) -> Result<Vec<Spawn>> {
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
            let number = |key: &str| -> Result<f64> {
                let n = fields
                    .get(key)
                    .map(|s| s.parse::<f64>())
                    .transpose()?
                    .unwrap_or(0.0);
                ensure!(
                    n.is_finite() && n.abs() < 1_000_000.0,
                    "invalid spawn coordinate"
                );
                Ok(n)
            };
            let spawn = Spawn {
                name: fields.get("name").cloned().unwrap_or(name),
                pos: [number("posx")?, number("posz")?, number("posy")?],
                heading: number("roty")?,
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

fn export(
    dir: &Path,
    installed_rel: &str,
    tiles: &Tiles,
    spawns: &[Spawn],
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let objects = dir.join("proton-import");
    let models = objects.join("model");
    std::fs::create_dir_all(&models)?;
    let mut global = format!("[name]\n{}\n[friendlyname]\n{} (Proton Bus)\n[version]\n14\n[description]\nBasic Proton Bus import: free driving only. AI, passengers, timetables and traffic lights are not imported.\n[end]\n[years]\n1980\n2100\n", installed_rel.rsplit('/').next().unwrap_or(installed_rel), installed_rel.trim_start_matches("maps/Proton-"));
    for (i, (&(tx, ty), mesh)) in tiles.iter().enumerate() {
        ensure!(!cancel(), "map import cancelled");
        let stem = format!("tile_{tx}_{ty}");
        let materials = &mesh.materials;
        convert::write_mesh(&models.join(format!("{stem}.o3d")), mesh, materials)?;
        // The engine probes a terrain even on tiles with only scenery. Keep that fallback
        // below the mesh: a zero-height fallback would cover Goianira's negative-height roads.
        let floor = mesh
            .vertices
            .iter()
            .map(|v| v.position.z)
            .fold(f32::INFINITY, f32::min)
            - 50.0;
        let mut terrain = 60u32.to_le_bytes().to_vec();
        for _ in 0..61 * 61 {
            terrain.extend_from_slice(&floor.to_le_bytes());
        }
        std::fs::write(dir.join(format!("{stem}.map.terrain")), terrain)?;
        let mut sco = format!("[absheight]\n[fixed]\n[surface]\n[mesh]\n{stem}.o3d\n");
        // Preparation has already removed unreferenced material slots.
        for (index, m) in materials.iter().enumerate() {
            // Basic PNG cutouts keep foliage and fences; explicit material opacity is blended.
            if m.diffuse[3] < 1.0 || m.texture.to_lowercase().ends_with(".png") {
                // The occurrence index names repeated textures, not the material slot.
                let occurrence = materials[..index]
                    .iter()
                    .filter(|other| other.texture.eq_ignore_ascii_case(&m.texture))
                    .count();
                sco.push_str(&format!(
                    "[matl]\n{}\n{occurrence}\n[matl_alpha]\n{}\n",
                    m.texture,
                    if m.diffuse[3] < 1.0 { 2 } else { 1 }
                ));
            }
        }
        std::fs::write(objects.join(format!("{stem}.sco")), sco)?;
        text(&dir.join(format!("{stem}.map")), &format!("[version]\n14\n[object]\n0\n{installed_rel}/proton-import/{stem}.sco\n{}\n0\n0\n0\n0\n0\n0\n0\n", i + 1))?;
        global.push_str(&format!("[map]\n{tx}\n{ty}\n{stem}.map\n"));
    }
    global.push_str(&format!("[entrypoints]\n{}\n", spawns.len()));
    for (i, spawn) in spawns.iter().enumerate() {
        let [x, y, z] = spawn.pos;
        let key = (
            (x / EDGE as f64).floor() as i32,
            (y / EDGE as f64).floor() as i32,
        );
        let group = tiles
            .keys()
            .position(|k| *k == key)
            .context("entry point is outside imported scenery")?;
        let half = spawn.heading.to_radians() * 0.5;
        // An id that has no scenery object makes the game use this explicit entry position.
        global.push_str(&format!(
            "{i}\n{}\n0\n{}\n{z}\n{}\n0\n{}\n0\n{}\n{group}\n{}\n",
            -(i as i64) - 1,
            x - key.0 as f64 * EDGE as f64,
            y - key.1 as f64 * EDGE as f64,
            half.sin(),
            half.cos(),
            spawn.name
        ));
    }
    text(&dir.join("global.cfg"), &global)?;
    Ok(())
}

/// `dir` is a fresh installer staging directory, not the source mod or installed map.
/// `installed_rel` is its eventual `maps/Proton-<name>` location below the content root.
pub fn import(dir: &Path, installed_rel: &str, cancel: impl Fn() -> bool) -> Result<Report> {
    ensure!(
        installed_rel.starts_with("maps/Proton-") && !installed_rel.contains(['\\', '\n', '\r']),
        "invalid imported map destination"
    );
    ensure!(
        !child(dir, "global.cfg")?.exists() && !child(dir, "proton-import")?.exists(),
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
    let config = ini(&configs[0])?;
    let map = config.get("map").context("missing Proton [map] section")?;
    let version = map
        .get("mapmodversion")
        .context("missing mapModVersion")?
        .parse::<u32>()?;
    ensure!(
        (2..=4).contains(&version),
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
    let mut spawns = spawns(&base, &models)?;
    let markers = prefixes(&models)?;
    let mut tiles = Tiles::new();
    let mut materials = Vec::new();
    let mut report = Report::default();
    for f in files(&models, "3ds")? {
        ensure!(!cancel(), "map import cancelled");
        ensure!(
            std::fs::metadata(&f)?.len() <= 128 << 20,
            "3DS file exceeds import limit"
        );
        let mut model = three_ds::parse(&std::fs::read(&f)?)
            .with_context(|| format!("reading {}", f.display()))?;
        if model.trailing_bytes != 0 {
            report.warnings.push(format!(
                "{}: ignored {} trailing bytes outside the declared 3DS root chunk",
                f.file_name().unwrap_or_default().to_string_lossy(),
                model.trailing_bytes
            ));
        }
        let all_gps = f
            .file_stem()
            .is_some_and(|s| optimise::gps_name(&s.to_string_lossy()));
        let before = model.objects.len();
        model
            .objects
            .retain(|o| !all_gps && !optimise::gps_name(&o.name));
        report.gps_skipped += before - model.objects.len();
        report.markers_skipped +=
            convert::append(model, &mut tiles, &mut materials, &markers, &cancel)?;
    }
    ensure!(
        !tiles.is_empty(),
        "Proton map has no renderable 3DS geometry"
    );
    spawn::validate(&tiles, &mut spawns, &mut report.warnings)?;
    optimise::used_materials(&mut tiles, &mut materials);
    textures(
        &texture_dir,
        &dir.join("proton-import/texture"),
        &mut materials,
        &mut report.warnings,
        &cancel,
    )?;
    report.vertices_removed = optimise::prepare(&mut tiles, &materials, &cancel)?;
    export(dir, installed_rel, &tiles, &spawns, &cancel)?;
    report.tiles = tiles.len();
    report.triangles = tiles.values().map(|m| m.triangles.len()).sum();
    report.warnings.sort();
    report.warnings.dedup();
    Ok(report)
}

#[cfg(test)]
mod tests;

//! OI envelopes for generated definitions; source assets and credits keep their bytes.
use anyhow::{ensure, Result};
use omsi_cfg::oi::{self, Kind};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn renamed(rel: &str) -> Option<(String, Kind)> {
    let (suffix, replacement, kind) = if rel.ends_with(".map.terrain") {
        (".map.terrain", ".terrainoi", Kind::Terrain)
    } else if rel.ends_with(".map") {
        (".map", ".tileoi", Kind::Tile)
    } else if rel.ends_with(".sco") {
        (".sco", ".oscoi", Kind::Scenery)
    } else if rel.ends_with(".o3d") {
        (".o3d", ".o3doi", Kind::Mesh)
    } else if rel.ends_with(".hof") {
        (".hof", ".hofoi", Kind::Depot)
    } else if rel.ends_with(".cfg") {
        (".cfg", ".cfgoi", Kind::Config)
    } else if rel.ends_with(".json") {
        (".json", ".jsonoi", Kind::Navigation)
    } else if rel.ends_with(".ovh") {
        (".ovh", ".ovhoi", Kind::Vehicle)
    } else if rel == "proton-import/depot.txt" {
        (".txt", ".cfgoi", Kind::Config)
    } else if rel == "proton-import/traffic.enabled" {
        (".enabled", ".cfgoi", Kind::Config)
    } else {
        return None;
    };
    Some((format!("{}{replacement}", rel.strip_suffix(suffix)?), kind))
}

fn generated(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir.join(rel))? {
        let entry = entry?;
        ensure!(!entry.file_type()?.is_symlink(), "link in generated output");
        let name = entry.file_name().to_string_lossy().into_owned();
        let name = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if entry.file_type()?.is_dir() {
            if name == "proton-import" || name.starts_with("proton-import/") {
                generated(dir, &name, out)?;
            }
        } else if (rel == "proton-import"
            || rel.starts_with("proton-import/")
            || name == "global.cfg"
            || (rel.is_empty()
                && name.starts_with("tile_")
                && (name.ends_with(".map") || name.ends_with(".map.terrain"))))
            && renamed(&name).is_some()
        {
            out.push(name);
        }
    }
    Ok(())
}

fn rewrite(text: &str, links: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut block = "";
    let mut offset = 0;
    for line in text.lines() {
        if line.starts_with('[') && line.ends_with(']') {
            block = &line[1..line.len() - 1];
            offset = 0;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let path_line = match block {
            "map" => offset == 2,
            "object" => offset == 1,
            "mesh" | "collision_mesh" | "passengercabin" | "paths" | "model" | "sound_ai" => {
                offset == 0
            }
            _ => false,
        };
        out.push_str(if path_line {
            links.get(line).map(String::as_str).unwrap_or(line)
        } else {
            line
        });
        out.push('\n');
        offset += 1;
    }
    out
}

pub(crate) fn seal(dir: &Path, installed: &str, cancel: &impl Fn() -> bool) -> Result<()> {
    let mut files = Vec::new();
    generated(dir, "", &mut files)?;
    let mut links = BTreeMap::new();
    for old in &files {
        let (new, _) = renamed(old).unwrap();
        for prefix in ["".to_string(), format!("{installed}/")] {
            links.insert(format!("{prefix}{old}"), format!("{prefix}{new}"));
        }
        if let (Some(a), Some(b)) = (Path::new(old).file_name(), Path::new(&new).file_name()) {
            links.insert(
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned(),
            );
        }
        if let Some(suffix) = old.strip_prefix("proton-import/ai/") {
            if let Some((_, rest)) = suffix.split_once('/') {
                links.insert(
                    rest.to_string(),
                    renamed(rest)
                        .map(|n| n.0)
                        .unwrap_or_else(|| rest.to_string()),
                );
            }
        }
    }
    for old in files {
        ensure!(!cancel(), "map import cancelled");
        let (new, kind) = renamed(&old).unwrap();
        let src = dir.join(&old);
        let target = dir.join(new);
        if matches!(
            kind,
            Kind::Config | Kind::Tile | Kind::Scenery | Kind::Vehicle
        ) && !old.ends_with("depot.txt")
            && !old.ends_with("traffic.enabled")
        {
            let text = omsi_cfg::decode_text(&std::fs::read(&src)?);
            crate::text(&src, &rewrite(&text, &links))?;
        }
        oi::encode_file(&src, &target, kind)?;
        std::fs::remove_file(src)?;
    }
    ensure!(dir.join(oi::GLOBAL).is_file(), "missing OI map manifest");
    Ok(())
}

/// Source-preserving CLI path using the same mandatory declaration as the launcher.
pub fn convert_folder(source: &Path, output: &Path, name: &str) -> Result<crate::Report> {
    ensure!(
        !output.exists(),
        "output already exists; choose a new folder"
    );
    let source = source.canonicalize()?;
    let configs: Vec<_> = crate::files(&source, "txt")?
        .into_iter()
        .filter(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".map.txt")
        })
        .collect();
    ensure!(
        configs.len() == 1,
        "select a folder containing exactly one .map.txt"
    );
    crate::permission::validate_file(&configs[0])?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    ensure!(
        !parent.starts_with(&source),
        "output must be outside the original map folder"
    );
    let label = crate::settings::label(name)?;
    ensure!(
        !label.is_empty() && !label.contains(['/', '\\', ':']) && label != "." && label != "..",
        "invalid output map name"
    );
    let stage = parent.join(format!(
        ".proton-convert-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&stage)?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let cleanup = Cleanup(stage.clone());
    let installed = format!("maps/Proton-{label}");
    let map = stage.join(&installed);
    std::fs::create_dir_all(&map)?;
    copy(&source, &map)?;
    let report = crate::import(&map, &installed, || false)?;
    let shared = stage.join("HOFs");
    std::fs::create_dir(&shared)?;
    std::fs::copy(
        map.join(crate::depot::FILE),
        shared.join(format!("{}.hofoi", crate::depot::name(&installed))),
    )?;
    std::fs::rename(&stage, output)?;
    drop(cleanup);
    Ok(report)
}

fn copy(source: &Path, target: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        ensure!(
            !entry.file_type()?.is_symlink(),
            "symbolic link in source map"
        );
        let dest = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir(&dest)?;
            copy(&entry.path(), &dest)?;
        } else if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

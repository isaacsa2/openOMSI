//! Native openOMSI/OMSI asset validation; generated definitions are not wrapped in OI envelopes.
//! Keep old OI readers for already-imported packages, but newly converted assets
//! use the engine's ordinary .cfg, .map, .sco, .o3d, .hof and .bus/.ovh loaders.
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

pub(crate) fn validate_native(
    dir: &Path,
    installed: &str,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let global = dir.join("global.cfg");
    ensure!(global.is_file(), "missing native map manifest");
    let map = omsi_map::GlobalCfg::load(&global).context("validating native map manifest")?;
    ensure!(!map.tiles.is_empty(), "native map has no tiles");
    for tile in &map.tiles {
        ensure!(!cancel(), "map import cancelled");
        ensure!(
            tile.file.starts_with("tile_")
                && tile.file.ends_with(".map")
                && !tile.file.split('/').any(|part| part == ".."),
            "generated tile is not a native map file"
        );
        let tile_path = dir.join(&tile.file);
        ensure!(tile_path.is_file(), "missing native tile {}", tile.file);
        let contents = omsi_map::Tile::load(&tile_path)
            .with_context(|| format!("validating native tile {}", tile.file))?;
        for object in contents.objects {
            let rel = object
                .file
                .strip_prefix(installed)
                .and_then(|p| p.strip_prefix('/'))
                .context("generated scenery path escapes the imported map")?;
            ensure!(
                !rel.is_empty()
                    && !rel.split('/').any(|part| part == "." || part == "..")
                    && !rel.contains(['\\', ':', '\0']),
                "unsafe generated scenery path"
            );
            ensure!(
                rel.ends_with(".sco"),
                "generated scenery has no native .sco extension"
            );
            let file = dir.join(rel);
            ensure!(
                file.is_file(),
                "missing generated scenery: {}",
                file.display()
            );
            ensure!(
                !std::fs::symlink_metadata(&file)?.file_type().is_symlink(),
                "generated scenery cannot be a symbolic link"
            );
        }
    }
    ensure!(
        dir.join(crate::depot::FILE).is_file(),
        "missing native depot"
    );
    ensure!(
        dir.join(crate::navigation::FILE).is_file(),
        "missing native navigation metadata"
    );
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
        shared.join(format!("{}.hof", crate::depot::name(&installed))),
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

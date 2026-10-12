//! Transactional vehicle conversion: public entry points always check the source declaration.
use crate::{permission, vehicle};
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, serde::Serialize)]
pub struct Report {
    pub meshes: usize,
    pub triangles: usize,
    pub seats: usize,
    pub standing: usize,
    pub doors: usize,
    pub sounds: usize,
    pub warnings: Vec<String>,
}

/// Convert one unpacked vehicle into a Mods package outside the source tree.
/// The original main configuration must carry the same declaration as a map.
/// No public ungated staging converter is exposed.
pub fn convert(file: &Path, output: &Path, cancel: impl Fn() -> bool) -> Result<Report> {
    permission::validate_file(file)?;
    let inspected = vehicle::inspect(file)?;
    ensure!(
        !output.exists(),
        "output already exists; choose a new folder"
    );
    let file = file.canonicalize()?;
    let source = file.parent().context("missing vehicle source folder")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    ensure!(
        !parent.starts_with(source),
        "output must be outside the original vehicle package"
    );
    ensure!(
        output.file_name().is_some_and(|n| n != "." && n != ".."),
        "invalid output folder"
    );
    let label = crate::settings::label(&inspected.name)?;
    ensure!(
        !label.is_empty() && !label.contains(['/', '\\', ':']) && label != "." && label != "..",
        "unsafe vehicle name"
    );
    ensure!(!cancel(), "vehicle conversion cancelled");
    let stage = parent.join(format!(
        ".proton-vehicle-{}-{}",
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
    let target = stage.join("Vehicles").join(format!("Proton-{label}"));
    std::fs::create_dir_all(&target)?;
    // Keep attribution and original assets together; never edit hard-linked sources.
    let copied = target.join("Source");
    std::fs::create_dir(&copied)?;
    copy(source, &copied, &cancel, 0, &mut 0)?;
    let main = copied.join(file.file_name().unwrap());
    permission::validate_file(&main)?;
    let inspected = vehicle::inspect(&main)?;
    let report = crate::vehicle_export::export(&main, &inspected, &target, &cancel)?;
    ensure!(!cancel(), "vehicle conversion cancelled");
    ensure!(!output.exists(), "output appeared during conversion");
    std::fs::rename(&stage, output)?;
    drop(cleanup);
    Ok(report)
}

fn copy(
    source: &Path,
    target: &Path,
    cancel: &impl Fn() -> bool,
    depth: usize,
    bytes: &mut u64,
) -> Result<()> {
    ensure!(
        depth <= 32,
        "vehicle package nesting exceeds 32 directories"
    );
    for entry in std::fs::read_dir(source)? {
        ensure!(!cancel(), "vehicle conversion cancelled");
        let entry = entry?;
        let ty = entry.file_type()?;
        ensure!(!ty.is_symlink(), "symbolic link in vehicle source package");
        let dest = target.join(entry.file_name());
        if ty.is_dir() {
            std::fs::create_dir(&dest)?;
            copy(&entry.path(), &dest, cancel, depth + 1, bytes)?;
        } else {
            ensure!(ty.is_file(), "non-regular vehicle source file");
            *bytes = bytes
                .checked_add(entry.metadata()?.len())
                .context("vehicle package size overflow")?;
            ensure!(
                *bytes <= 2 * 1024 * 1024 * 1024,
                "vehicle source package exceeds 2 GiB"
            );
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

pub(crate) fn text(path: &Path, text: &str, kind: omsi_cfg::oi::Kind) -> Result<()> {
    let raw = path.with_extension("temporary");
    crate::text(&raw, text)?;
    omsi_cfg::oi::encode_file(&raw, path, kind)?;
    std::fs::remove_file(raw)?;
    Ok(())
}

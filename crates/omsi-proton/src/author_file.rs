//! Files an author writes beside a map's configuration (`<map>.itineraries.txt`,
//! `<map>.destinations.txt`): OpenOMSI files, not Proton ones, so the original configuration is
//! never edited and a map without them imports exactly as before.
use anyhow::{ensure, Result};
use std::path::Path;

const FILE_MAX: u64 = 1 << 20;

/// The bytes of `<map><suffix>` beside `config` (`<map>.map.txt`), found whatever the case of
/// its name. `None` when there is no such file.
pub(crate) fn read(config: &Path, suffix: &str) -> Result<Option<Vec<u8>>> {
    let Some(name) = config.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Ok(None);
    };
    let stem = name
        .get(..name.len().saturating_sub(".map.txt".len()))
        .unwrap_or(&name);
    let wanted = format!("{stem}{suffix}");
    let Some(folder) = config.parent() else {
        return Ok(None);
    };
    let mut found = None;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(&wanted)
        {
            ensure!(
                found.is_none(),
                "more than one {wanted} (they differ in case only)"
            );
            ensure!(
                entry.file_type()?.is_file(),
                "{wanted} is not a regular file"
            );
            found = Some(entry.path());
        }
    }
    let Some(path) = found else {
        return Ok(None);
    };
    ensure!(
        std::fs::metadata(&path)?.len() <= FILE_MAX,
        "{wanted} exceeds 1 MiB"
    );
    Ok(Some(std::fs::read(&path)?))
}

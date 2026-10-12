//! Basic textual destinations; route images do not encode an OMSI timetable.
use crate::{Report, Spawn, child, settings::label, stops::Stop};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
    path::Path,
};

/// Generated HOF kept with the map; the installer also publishes it in shared HOFs/.
pub const FILE: &str = "proton-import/depot.hofoi";
/// Display name of the shared HOF, used by map-specific depot selection.
pub const NAME_FILE: &str = "proton-import/depot.cfgoi";

/// Portable, map-specific name: no source path is interpolated into shared HOFs/.
pub fn name(installed: &str) -> String {
    let hash = installed.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b.to_ascii_lowercase())).wrapping_mul(0x100000001b3)
    });
    format!("Proton-{hash:016x}")
}

/// Read the small display-name hint without loading all HOF destinations.
pub fn installed_name(dir: &Path) -> Option<String> {
    let file = dir.join(NAME_FILE);
    if omsi_cfg::vfs::file_size(&file)? > 4096 { return None; }
    let name = omsi_cfg::vfs::read_text(&file).ok()?;
    (!name.is_empty() && name.len() <= 1024 && !name.chars().any(char::is_control)).then_some(name)
}

fn field(s: &str) -> Result<String> {
    Ok(label(s)?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect())
}

// Write bounded records directly: do not hold a complete HOF and its UTF-16 copy
// in RAM alongside the imported map's gameplay metadata.
fn write(out: &mut impl Write, s: &str) -> Result<()> {
    for c in s.replace('\n', "\r\n").encode_utf16() {
        out.write_all(&c.to_le_bytes())?;
    }
    Ok(())
}

pub(crate) fn export(
    dir: &Path,
    base: &Path,
    installed: &str,
    spawns: &[Spawn],
    stops: &[Stop],
    report: &mut Report,
) -> Result<()> {
    let mut destinations = BTreeMap::new();
    for spawn in spawns {
        destinations.insert(field(&spawn.name)?, false);
        ensure!(destinations.len() <= 4096, "too many map destinations");
    }
    let dest = child(base, "dest")?;
    if dest.is_dir() {
        for entry in std::fs::read_dir(&dest)? {
            let entry = entry?;
            // Do not follow links or import files as route folders.
            ensure!(
                !entry.file_type()?.is_symlink(),
                "symbolic link in destination folder"
            );
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = child(&dest, &entry.file_name().to_string_lossy())?;
            let reserved = child(&path, "outofservice.txt")?.is_file();
            destinations.insert(field(&entry.file_name().to_string_lossy())?, reserved);
            ensure!(destinations.len() <= 4096, "too many map destinations");
        }
    }
    // Explicit terminal markers provide valid unload destinations, but do not tell
    // us which entrypoint serves them or the ordered stops of a line.
    for stop in stops.iter().filter(|s| s.terminal) {
        destinations.entry(field(&stop.name)?).or_insert(false);
    }
    destinations.retain(|name, _| !name.trim().is_empty());
    ensure!(destinations.len() <= 8192, "too many map destinations");
    let depot = field(&format!(
        "{} (Proton Bus)",
        installed.trim_start_matches("maps/Proton-")
    ))?;
    let mut out = BufWriter::new(std::fs::File::create(dir.join("proton-import/depot.hof"))?);
    out.write_all(&[0xff, 0xfe])?;
    write(
        &mut out,
        &format!(
            "[name]\n{depot}\n\nstringcount_terminus\n6\nstringcount_busstop\n3\n\n[addterminus_list]\n"
        ),
    )?;
    // Codes are generated indices, not the Proton route number or an IBIS route.
    for (i, (name, reserved)) in destinations.iter().enumerate() {
        let flag = if *reserved { "{ALLEX}" } else { "" };
        // Text-only layout: no fabricated roll-sign bitmap filename.
        write(
            &mut out,
            &format!(
                "{flag}\t{}\t{name}\t{name}\t{name}\t\t{name}\t\t{name}\n",
                i + 1
            ),
        )?;
    }
    write(&mut out, "[end]\n\n[addbusstop_list]\n")?;
    let names: std::collections::BTreeSet<_> = stops
        .iter()
        .map(|s| field(&s.name))
        .collect::<Result<_>>()?;
    for name in names {
        write(&mut out, &format!("{name}\t{name}\t{name}\t{name}\n"))?;
    }
    write(&mut out, "[end]\n")?;
    out.flush()?;
    std::fs::write(dir.join("proton-import/depot.txt"), &depot)?;
    report.destinations_imported = destinations.len();
    Ok(())
}

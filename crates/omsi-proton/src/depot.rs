//! Basic textual destinations; route images do not encode an OMSI timetable.
use crate::{Report, Spawn, child, destination_text::Signs, settings::label, stops::Stop};
use anyhow::{Result, ensure};
use std::{
    cmp::Ordering,
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

/// A destination the generated HOF offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Destination {
    /// `outofservice.txt`, or the author's `out_of_service`: everybody leaves the bus.
    pub reserved: bool,
    /// An entry point or a `dest/` folder: a service, named for its entry point (where it
    /// begins). Otherwise the destination exists only because a stop is marked as the latest
    /// stop, and the line ends there.
    pub service: bool,
}

pub(crate) type Destinations = BTreeMap<String, Destination>;

/// Add a service. One that differs from an earlier service only in case (a `dest/` folder
/// spelt unlike its entry point) is that service, not a second destination: the menu would
/// show the name twice and a line could not say which it means.
fn service(
    found: &mut Destinations,
    spelt: &mut BTreeMap<String, String>,
    warnings: &mut Vec<String>,
    name: String,
    reserved: Option<bool>,
    from: &str,
) {
    match spelt.get(&name.to_lowercase()) {
        Some(kept) => {
            if *kept != name {
                warnings.push(format!(
                    "{from} {name:?} differs only in case from the destination {kept:?}; they are one destination"
                ));
            }
            if let Some(d) = found.get_mut(kept) {
                if let Some(reserved) = reserved {
                    d.reserved = reserved;
                }
            }
        }
        None => {
            spelt.insert(name.to_lowercase(), name.clone());
            found.insert(name, Destination { reserved: reserved.unwrap_or(false), service: true });
        }
    }
}

/// Every destination the generated HOF offers: entry points, `dest/` folders (those with
/// `outofservice.txt` are reserved) and the stops marked as the latest stop.
pub(crate) fn destinations(
    base: &Path,
    spawns: &[Spawn],
    stops: &[Stop],
    warnings: &mut Vec<String>,
) -> Result<Destinations> {
    let mut destinations = Destinations::new();
    let mut spelt = BTreeMap::new();
    for spawn in spawns {
        service(&mut destinations, &mut spelt, warnings, field(&spawn.name)?, None, "entry point");
        ensure!(destinations.len() <= 4096, "too many map destinations");
    }
    let dest = child(base, "dest")?;
    if dest.is_dir() {
        // Sorted, so a spelling clash is always settled the same way.
        let mut folders = Vec::new();
        for entry in std::fs::read_dir(&dest)? {
            let entry = entry?;
            // Do not follow links or import files as route folders.
            ensure!(
                !entry.file_type()?.is_symlink(),
                "symbolic link in destination folder"
            );
            if entry.file_type()?.is_dir() {
                folders.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        folders.sort();
        for folder in folders {
            let path = child(&dest, &folder)?;
            let reserved = child(&path, "outofservice.txt")?.is_file();
            service(&mut destinations, &mut spelt, warnings, field(&folder)?, Some(reserved), "dest/ folder");
            ensure!(destinations.len() <= 4096, "too many map destinations");
        }
    }
    // Explicit terminal markers provide valid unload destinations, but do not tell
    // us which entrypoint serves them or the ordered stops of a line. A stop keeps its exact
    // name: it is also the stop where everybody leaves.
    for stop in stops.iter().filter(|s| s.terminal) {
        destinations
            .entry(field(&stop.name)?)
            .or_insert(Destination { reserved: false, service: false });
    }
    destinations.retain(|name, _| !name.trim().is_empty());
    ensure!(destinations.len() <= 8192, "too many map destinations");
    Ok(destinations)
}

/// Numbers compare as numbers and letters without regard to case: 44 comes before 420AT and
/// 2 before 10, the way a driver reads a list of lines.
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |s: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = s.next_if(char::is_ascii_digit) {
                        digits.push(c);
                    }
                    digits
                };
                let (x, y) = (run(&mut a), run(&mut b));
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                match x.len().cmp(&y.len()).then_with(|| x.cmp(y)) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (Some(x), Some(y)) => {
                let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
                match fold(x).cmp(&fold(y)) {
                    Ordering::Equal => {
                        a.next();
                        b.next();
                    }
                    other => return other,
                }
            }
        }
    }
}

/// The services first, by name, then the stops that only end a line: a driver picking a
/// destination finds the lines together, and codes of services do not move when stops come
/// or go.
fn ordered(destinations: &Destinations) -> Vec<(&String, &Destination)> {
    let mut all: Vec<_> = destinations.iter().collect();
    all.sort_by(|(a, x), (b, y)| {
        (!x.service)
            .cmp(&!y.service)
            .then_with(|| natural(a, b))
            .then_with(|| a.cmp(b))
    });
    all
}

pub(crate) fn export(
    dir: &Path,
    installed: &str,
    destinations: &Destinations,
    signs: &Signs,
    stops: &[Stop],
    report: &mut Report,
) -> Result<()> {
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
    for (i, (name, destination)) in ordered(destinations).into_iter().enumerate() {
        let flag = if destination.reserved { "{ALLEX}" } else { "" };
        // A destination the author gave no text keeps the layout it always had. One that has
        // one shows it on the first line, the author's second line (or nothing) on the
        // second, and the first line again on the side and rear signs, as the stock files do.
        let (first, second) = match signs.get(name) {
            Some(sign) if sign.text.is_some() || sign.via.is_some() => (
                field(sign.text.as_deref().unwrap_or(name))?,
                field(sign.via.as_deref().unwrap_or(""))?,
            ),
            _ => (name.clone(), name.clone()),
        };
        // Text-only layout: no fabricated roll-sign bitmap filename.
        write(
            &mut out,
            &format!(
                "{flag}\t{}\t{name}\t{first}\t{second}\t\t{first}\t\t{first}\n",
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

#[cfg(test)]
mod order_tests {
    use super::natural;
    use std::cmp::Ordering::{Greater, Less};

    #[test]
    fn numbers_count_as_numbers_and_case_is_ignored() {
        assert_eq!(natural("44 METACIL", "420AT TREVO"), Less);
        assert_eq!(natural("2 Rota", "10 Rota"), Less);
        assert_eq!(natural("490 TERMINAL", "490EX JD"), Less);
        assert_eq!(natural("490EX", "494 LAVRAS"), Less);
        assert_eq!(natural("721 A", "820 B"), Less);
        assert_eq!(natural("garagem", "GARAGEM COOPER"), Less);
        assert_eq!(natural("Zeta", "alfa"), Greater);
        assert_eq!(natural("12 a", "012 a"), std::cmp::Ordering::Equal, "leading zeros are no difference");
        assert_eq!(natural("", "a"), Less);
        // far longer than any integer type: still compared as numbers
        assert_eq!(natural("99999999999999999999999 a", "100000000000000000000000 a"), Less);
    }
}

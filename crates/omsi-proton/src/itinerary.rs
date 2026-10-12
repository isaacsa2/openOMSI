//! Itineraries an author declares: a line's direction, destination and ordered stops.
//!
//! A Proton package carries no stop sequence, and none is deduced here from names, route
//! polygons, entry points or nearness. An itinerary exists only when the author writes it in
//! `<map>.itineraries.txt` beside `<map>.map.txt` (an OpenOMSI file, not a Proton one: the
//! original configuration is never edited):
//!
//! ```ini
//! [itinerary]
//! line=490
//! direction=Centro
//! destination=490 TERMINAL PIMENTAS
//! stop=Praca Central
//! stop=@busstop_17
//! stop=Terminal Pimentas
//! ```
//!
//! `line` and `direction` are optional text kept as written (a generated HOF index is never a
//! line number). `destination` must be one of the generated HOF's destinations. Two or more
//! `stop` keys name the stops in order: a display name that belongs to exactly one imported
//! stop, or `@` and the stop's file name in `busstops/` to tell apart stops that share a name.
//! Comments are whole lines starting with `;` or `#`.
//!
//! The file is read strictly: a line that is not understood, a repeated key, a missing section
//! or a size over the limits stops the import and names the line, since guessing what the
//! author meant would invent a service. An itinerary whose stops or destination cannot be
//! resolved to exactly one thing is left out and reported instead, so one mistake does not
//! take the others with it.
use crate::{gameplay::PlacedStop, settings::label, Report};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

/// Generated next to the navigation data.
pub const FILE: &str = "proton-import/itineraries.json";
/// What the installed map holds once its generated files are sealed.
pub const SEALED: &str = "proton-import/itineraries.jsonoi";

const FILE_MAX: u64 = 1 << 20;
const MAX_ITINERARIES: usize = 512;
const MAX_STOPS: usize = 512;
const MAX_REFERENCES: usize = 20_000;
const MAX_TEXT: usize = 256;

/// An imported stop as an itinerary can name it.
#[derive(Debug)]
pub(crate) struct Known {
    pub id: String,
    pub name: String,
}

#[derive(Default, Debug, PartialEq)]
struct Raw {
    at: usize,
    line: Option<String>,
    direction: Option<String>,
    destination: Option<String>,
    stops: Vec<String>,
}

/// One stop of an itinerary, as the converted map holds it.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ItineraryStop {
    /// The stop's native object id in the converted map.
    pub object: usize,
    /// The stop's file name in `busstops/`, unique in the map.
    pub id: String,
    pub name: String,
}

/// An itinerary whose stops are all resolved to one imported stop each. The last stop is
/// the terminal; `destination` is a destination of the generated HOF.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Itinerary {
    pub line: Option<String>,
    pub direction: Option<String>,
    pub destination: String,
    pub stops: Vec<ItineraryStop>,
}

#[derive(Serialize)]
struct Output<'a> {
    version: u32,
    itineraries: &'a [Itinerary],
}

#[derive(Deserialize)]
struct Input {
    version: u32,
    itineraries: Vec<Itinerary>,
}

/// Read the itineraries of a converted map (the decoded bytes of [`SEALED`]). Anything out
/// of bounds, of another version or with fewer than two stops is refused as a whole.
pub fn from_bytes(bytes: &[u8]) -> Result<Vec<Itinerary>> {
    ensure!(bytes.len() <= 8 << 20, "itineraries file exceeds 8 MiB");
    let input: Input = serde_json::from_slice(bytes)?;
    ensure!(input.version == 1, "unsupported itineraries version {}", input.version);
    ensure!(input.itineraries.len() <= MAX_ITINERARIES, "too many itineraries");
    for i in &input.itineraries {
        ensure!(
            (2..=MAX_STOPS).contains(&i.stops.len()) && !i.destination.is_empty(),
            "invalid itinerary to {:?}",
            i.destination
        );
    }
    Ok(input.itineraries)
}

fn text(n: usize, key: &str, value: &str) -> Result<String> {
    ensure!(!value.is_empty(), "itineraries file, line {n}: {key} is empty");
    ensure!(
        value.chars().count() <= MAX_TEXT && !value.chars().any(char::is_control),
        "itineraries file, line {n}: {key} is too long or holds a control character"
    );
    label(value)
}

fn once(slot: &mut Option<String>, n: usize, key: &str, value: &str) -> Result<()> {
    ensure!(slot.is_none(), "itineraries file, line {n}: {key} is given twice in one [itinerary]");
    *slot = Some(text(n, key, value)?);
    Ok(())
}

fn parse(bytes: &[u8]) -> Result<Vec<Raw>> {
    let mut out: Vec<Raw> = Vec::new();
    let mut references = 0;
    for (i, line) in crate::ini_text(bytes).trim_start_matches('\u{feff}').lines().enumerate() {
        let n = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            ensure!(
                name.trim().eq_ignore_ascii_case("itinerary"),
                "itineraries file, line {n}: unknown section [{name}]"
            );
            ensure!(out.len() < MAX_ITINERARIES, "itineraries file: more than {MAX_ITINERARIES} itineraries");
            out.push(Raw { at: n, ..Default::default() });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            bail!("itineraries file, line {n}: expected key=value");
        };
        let Some(current) = out.last_mut() else {
            bail!("itineraries file, line {n}: {} is outside an [itinerary] section", key.trim());
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match key.as_str() {
            "line" => once(&mut current.line, n, "line", value)?,
            "direction" => once(&mut current.direction, n, "direction", value)?,
            "destination" => once(&mut current.destination, n, "destination", value)?,
            "stop" => {
                ensure!(current.stops.len() < MAX_STOPS, "itineraries file, line {n}: more than {MAX_STOPS} stops");
                references += 1;
                ensure!(references <= MAX_REFERENCES, "itineraries file: too many stops in all");
                current.stops.push(text(n, "stop", value)?);
            }
            other => bail!("itineraries file, line {n}: unknown key {other}"),
        }
    }
    Ok(out)
}

/// The one stop a reference names, or why it names none.
fn resolve_stop<'a>(known: &'a [Known], reference: &str) -> Result<&'a Known, String> {
    let (wanted, by_id) = match reference.strip_prefix('@') {
        Some(id) => (id, true),
        None => (reference, false),
    };
    let hits: Vec<&Known> = known
        .iter()
        .filter(|k| if by_id { &k.id } else { &k.name }.eq_ignore_ascii_case(wanted))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("{reference:?} is no imported stop")),
        many => Err(format!(
            "{reference:?} names {} stops; give one as {}",
            many.len(),
            many.iter().map(|k| format!("@{}", k.id)).collect::<Vec<_>>().join(" or ")
        )),
    }
}

/// An itinerary resolved to stop ids, or the reason it cannot be.
fn resolve(raw: &Raw, known: &[Known], destinations: &BTreeMap<String, bool>) -> Result<Draft, String> {
    let wanted = raw.destination.as_deref().ok_or("no destination")?;
    if raw.stops.len() < 2 {
        return Err("an itinerary needs at least two stops".into());
    }
    let hits: Vec<(&String, &bool)> = destinations.iter().filter(|(d, _)| d.eq_ignore_ascii_case(wanted)).collect();
    let destination = match hits.as_slice() {
        [(name, false)] => (*name).clone(),
        [(_, true)] => return Err(format!("{wanted:?} is an out-of-service destination")),
        [] => return Err(format!("{wanted:?} is no destination of the generated HOF")),
        _ => return Err(format!("{wanted:?} matches several destinations of the generated HOF")),
    };
    let mut stops: Vec<&Known> = Vec::with_capacity(raw.stops.len());
    for reference in &raw.stops {
        let stop = resolve_stop(known, reference)?;
        if stops.last().is_some_and(|last| last.id == stop.id) {
            return Err(format!("{reference:?} is named twice in a row"));
        }
        stops.push(stop);
    }
    // A destination that is also a stop's name is the terminal: it has to be the last stop.
    let last = stops.last().expect("two stops at least");
    if known.iter().any(|k| k.name.eq_ignore_ascii_case(&destination)) && !last.name.eq_ignore_ascii_case(&destination) {
        return Err(format!("the destination {destination:?} is a stop, but the last stop is {:?}", last.name));
    }
    Ok(Draft {
        line: raw.line.clone(),
        direction: raw.direction.clone(),
        destination,
        stops: stops.into_iter().map(|k| (k.id.clone(), k.name.clone())).collect(),
    })
}

/// An itinerary before the map is written: its stops are source ids, not native objects yet.
pub(crate) struct Draft {
    line: Option<String>,
    direction: Option<String>,
    destination: String,
    stops: Vec<(String, String)>,
}

impl Draft {
    fn key(&self) -> (String, String, String) {
        let lower = |s: &Option<String>| s.as_deref().unwrap_or("").to_lowercase();
        (lower(&self.line), lower(&self.direction), self.destination.to_lowercase())
    }
}

/// Resolve every itinerary; the ones that cannot be are reported and left out. Two with the
/// same line, direction and destination are one when their stops agree and both are left out
/// when they differ: which of them the bus runs would be a guess.
fn drafts(raws: &[Raw], known: &[Known], destinations: &BTreeMap<String, bool>, report: &mut Report) -> Vec<Draft> {
    let mut out: Vec<(usize, Draft)> = Vec::new();
    for raw in raws {
        match resolve(raw, known, destinations) {
            Ok(d) => out.push((raw.at, d)),
            Err(why) => report.warnings.push(format!("itineraries file, line {}: itinerary left out: {why}", raw.at)),
        }
    }
    let mut kept: Vec<(usize, Draft)> = Vec::new();
    // (line, key) of every itinerary that disagrees with an earlier one of its kind
    let mut late: Vec<(usize, (String, String, String))> = Vec::new();
    for (at, draft) in out {
        match kept.iter().find(|(_, k)| k.key() == draft.key()) {
            Some((_, k)) if k.stops == draft.stops => report.warnings.push(format!("itineraries file, line {at}: the same itinerary is given again; the first is kept")),
            Some(_) => late.push((at, draft.key())),
            None => kept.push((at, draft)),
        }
    }
    let doubt = |at: usize| format!("itineraries file, line {at}: itinerary left out: another with the same line, direction and destination has other stops; tell them apart by line or direction");
    for (at, _) in &late {
        report.warnings.push(doubt(*at));
    }
    kept.retain(|(at, d)| {
        let conflicting = late.iter().any(|(_, key)| *key == d.key());
        if conflicting {
            report.warnings.push(doubt(*at));
        }
        !conflicting
    });
    kept.into_iter().map(|(_, d)| d).collect()
}

/// The author's itineraries for the map whose configuration is `config`, resolved against the
/// imported stops and the generated HOF's destinations. No file means no itineraries.
pub(crate) fn load(
    config: &Path,
    known: &[Known],
    destinations: &BTreeMap<String, bool>,
    report: &mut Report,
) -> Result<Vec<Draft>> {
    let Some(name) = config.file_name().map(|n| n.to_string_lossy().into_owned()) else { return Ok(Vec::new()) };
    let wanted = format!("{}.itineraries.txt", &name[..name.len().saturating_sub(".map.txt".len())]);
    let Some(folder) = config.parent() else { return Ok(Vec::new()) };
    let mut found = None;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case(&wanted) {
            ensure!(found.is_none(), "more than one {wanted} (they differ in case only)");
            ensure!(entry.file_type()?.is_file(), "{wanted} is not a regular file");
            found = Some(entry.path());
        }
    }
    let Some(path) = found else { return Ok(Vec::new()) };
    ensure!(std::fs::metadata(&path)?.len() <= FILE_MAX, "{wanted} exceeds 1 MiB");
    let raws = parse(&std::fs::read(&path)?)?;
    Ok(drafts(&raws, known, destinations, report))
}

/// Write the itineraries whose stops were all placed in the map, with their native object ids.
pub(crate) fn export(dir: &Path, drafts: Vec<Draft>, placed: &[PlacedStop], report: &mut Report) -> Result<()> {
    let mut out = Vec::new();
    'draft: for d in drafts {
        let mut stops = Vec::with_capacity(d.stops.len());
        for (id, name) in &d.stops {
            let Some(p) = placed.iter().find(|p| &p.id == id) else {
                report.warnings.push(format!("itinerary to {:?} left out: stop {name:?} is not in the converted map", d.destination));
                continue 'draft;
            };
            stops.push(ItineraryStop { object: p.object, id: id.clone(), name: name.clone() });
        }
        out.push(Itinerary { line: d.line, direction: d.direction, destination: d.destination, stops });
    }
    report.itineraries_imported = out.len();
    if out.is_empty() {
        return Ok(());
    }
    let file = std::fs::File::create(dir.join(FILE))?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &Output { version: 1, itineraries: &out })?;
    std::io::Write::flush(&mut writer)?;
    Ok(())
}

#[cfg(test)]
#[path = "itinerary_tests.rs"]
mod tests;

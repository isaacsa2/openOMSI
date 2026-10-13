//! Sign texts an author declares for the generated HOF's destinations.
//!
//! A Proton map keeps what a destination sign says only in images (`dest/<name>/`), and no text
//! is read back from them. A bus display shows the HOF's text, which on its own is the name of
//! the destination: for a service that is the name of its entry point, not what the sign should
//! say. When the author wants the displays to say where a service goes, they write it in
//! `<map>.destinations.txt` beside `<map>.map.txt` (an OpenOMSI file, not a Proton one: the
//! original configuration is never edited):
//!
//! ```ini
//! [destination]
//! name=490 TERMINAL PIMENTAS
//! text=TERM. SAO JOAO
//! via=VIA LAVRAS
//! out_of_service=0
//! ```
//!
//! `name` is a destination of the generated HOF, compared without regard to case. `text` is
//! what the first line of its displays shows and `via` the second line; either may be left out.
//! `out_of_service=1` marks a destination everybody leaves at (a garage run), as an
//! `outofservice.txt` in its `dest/` folder does, and `0` clears that mark. Nothing is guessed
//! from the images or the names: a destination the file does not mention keeps its name as
//! its text. Comments are whole lines starting with `;` or `#`.
//!
//! The file is read strictly: a line that is not understood, a repeated key, a value that is
//! empty, too long or holds a control character, or a size over the limits stops the import
//! and names the line. A `[destination]` whose name is no destination of the HOF, or that
//! gives nothing, is left out and reported, so one mistake does not take the others with it.
use crate::{depot::Destinations, settings::label, Report};
use anyhow::{bail, ensure, Result};
use std::{collections::BTreeMap, path::Path};

const MAX_ENTRIES: usize = 4096;
const MAX_TEXT: usize = 256;

/// What the author declared for one destination.
#[derive(Debug, Default, PartialEq, Clone)]
pub(crate) struct Sign {
    pub text: Option<String>,
    pub via: Option<String>,
    pub out_of_service: Option<bool>,
}

/// The declared signs, by the exact name of the destination they belong to.
pub(crate) type Signs = BTreeMap<String, Sign>;

#[derive(Default, Debug, PartialEq)]
struct Raw {
    at: usize,
    name: Option<String>,
    sign: Sign,
}

fn text(n: usize, key: &str, value: &str) -> Result<String> {
    ensure!(
        !value.is_empty(),
        "destinations file, line {n}: {key} is empty"
    );
    ensure!(
        value.chars().count() <= MAX_TEXT && !value.chars().any(char::is_control),
        "destinations file, line {n}: {key} is too long or holds a control character"
    );
    label(value)
}

fn once(slot: &mut Option<String>, n: usize, key: &str, value: &str) -> Result<()> {
    ensure!(
        slot.is_none(),
        "destinations file, line {n}: {key} is given twice in one [destination]"
    );
    *slot = Some(text(n, key, value)?);
    Ok(())
}

fn parse(bytes: &[u8]) -> Result<Vec<Raw>> {
    let mut out: Vec<Raw> = Vec::new();
    for (i, line) in crate::ini_text(bytes)
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
    {
        let n = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            ensure!(
                name.trim().eq_ignore_ascii_case("destination"),
                "destinations file, line {n}: unknown section [{name}]"
            );
            ensure!(
                out.len() < MAX_ENTRIES,
                "destinations file: more than {MAX_ENTRIES} destinations"
            );
            out.push(Raw {
                at: n,
                ..Default::default()
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            bail!("destinations file, line {n}: expected key=value");
        };
        let Some(current) = out.last_mut() else {
            bail!(
                "destinations file, line {n}: {} is outside a [destination]",
                key.trim()
            );
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match key.as_str() {
            "name" => once(&mut current.name, n, "name", value)?,
            "text" => once(&mut current.sign.text, n, "text", value)?,
            "via" => once(&mut current.sign.via, n, "via", value)?,
            "out_of_service" => {
                ensure!(
                    current.sign.out_of_service.is_none(),
                    "destinations file, line {n}: out_of_service is given twice in one [destination]"
                );
                current.sign.out_of_service = Some(match value {
                    "0" => false,
                    "1" => true,
                    _ => bail!("destinations file, line {n}: out_of_service is 0 or 1"),
                });
            }
            other => bail!("destinations file, line {n}: unknown key {other}"),
        }
    }
    Ok(out)
}

/// The destination a `[destination]` names: the exact spelling, else the one that differs from
/// it in case only.
fn resolve<'a>(raw: &Raw, destinations: &'a Destinations) -> Result<&'a String, String> {
    let wanted = raw.name.as_deref().ok_or("it has no name")?;
    if raw.sign == Sign::default() {
        return Err("it gives no text, via or out_of_service".into());
    }
    if let Some((exact, _)) = destinations.get_key_value(wanted) {
        return Ok(exact);
    }
    let hits: Vec<&String> = destinations
        .keys()
        .filter(|d| d.eq_ignore_ascii_case(wanted))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("{wanted:?} is no destination of the generated HOF")),
        _ => Err(format!(
            "{wanted:?} matches several destinations of the generated HOF; spell it exactly"
        )),
    }
}

/// Resolve every `[destination]`. Two for one destination are one when they agree, and both
/// are left out when they differ: which of them the displays show would be a guess.
fn signs(raws: &[Raw], destinations: &Destinations, report: &mut Report) -> Signs {
    let mut found: Vec<(usize, &String, &Sign)> = Vec::new();
    for raw in raws {
        match resolve(raw, destinations) {
            Ok(name) => found.push((raw.at, name, &raw.sign)),
            Err(why) => report.warnings.push(format!(
                "destinations file, line {}: destination left out: {why}",
                raw.at
            )),
        }
    }
    let mut out = Signs::new();
    for (at, name, sign) in &found {
        let rivals: Vec<_> = found
            .iter()
            .filter(|(other, n, _)| n == name && other != at)
            .collect();
        if rivals.iter().any(|(_, _, s)| *s != *sign) {
            report.warnings.push(format!(
                "destinations file, line {at}: destination {name:?} left out: it is declared again with other values; give it once"
            ));
        } else if rivals.iter().any(|(other, _, _)| other < at) {
            report.warnings.push(format!(
                "destinations file, line {at}: destination {name:?} is declared again; the first is kept"
            ));
        } else {
            out.insert((*name).clone(), (*sign).clone());
        }
    }
    out
}

/// The author's signs for the map whose configuration is `config`, resolved against the
/// generated HOF's destinations. No file means no signs.
pub(crate) fn load(
    config: &Path,
    destinations: &Destinations,
    report: &mut Report,
) -> Result<Signs> {
    let Some(bytes) = crate::author_file::read(config, ".destinations.txt")? else {
        return Ok(Signs::new());
    };
    let signs = signs(&parse(&bytes)?, destinations, report);
    report.destination_signs_imported = signs.len();
    Ok(signs)
}

/// Mark the destinations the author declared out of service (or in service again).
pub(crate) fn apply(signs: &Signs, destinations: &mut Destinations) {
    for (name, sign) in signs {
        if let (Some(reserved), Some(d)) = (sign.out_of_service, destinations.get_mut(name)) {
            d.reserved = reserved;
        }
    }
}

#[cfg(test)]
#[path = "destination_text_tests.rs"]
mod tests;

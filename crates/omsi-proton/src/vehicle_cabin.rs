//! Hidden seat/path markers become a native passenger cabin, never visible boxes.
use crate::{
    settings::{number, position},
    three_ds::Object,
    vehicle::Inspection,
    Ini,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Cabin {
    places: Vec<(Vec3, bool, f32)>,
    routes: BTreeMap<(String, char), BTreeMap<usize, Vec3>>,
    pub entries: Vec<usize>,
    pub exits: Vec<usize>,
}
impl Cabin {
    pub fn marker(&mut self, o: &Object) -> Result<bool> {
        let name = o.name.trim().to_ascii_lowercase();
        if let Some((side, group, index)) = path_tag(&name) {
            ensure!(
                self.routes.len() < 128 && index <= 512,
                "too many passenger path markers"
            );
            let p = center(o)?;
            let previous = self
                .routes
                .entry((side, group))
                .or_default()
                .insert(index, p);
            ensure!(
                previous.is_none_or(|old| old.distance(p) < 0.02),
                "conflicting passenger path marker {}",
                o.name
            );
            return Ok(true);
        }
        if name.contains("_sit_") || name.contains("_stand_") {
            ensure!(self.places.len() < 512, "too many passenger places");
            let p = center(o)?;
            let sitting = name.contains("_sit_");
            let rot = if name.contains("_rot-90_") {
                -90.
            } else if name.contains("_rot90_") {
                90.
            } else if name.contains("_rot180_") {
                180.
            } else {
                0.
            };
            if !self
                .places
                .iter()
                .any(|(v, s, r)| *s == sitting && *r == rot && v.distance(p) < 0.02)
            {
                self.places.push((p, sitting, rot));
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub fn definitions(
        &mut self,
        info: &Inspection,
        cfg: &Ini,
        report: &mut crate::vehicle_package::Report,
    ) -> Result<(String, String)> {
        if let Some(file) = info
            .components
            .iter()
            .find(|c| c.role == "seats1" && c.present)
        {
            self.places.clear();
            let seats = crate::ini(&file.file)?;
            for (prefix, sit) in [("sitting", true), ("standing", false)] {
                for i in 1..=512 {
                    let section = format!("{prefix}{i}");
                    if !seats.contains_key(&section) {
                        break;
                    }
                    self.places.push((
                        Vec3::from_array(position(&seats, &section)?),
                        sit,
                        number(&seats, &section, "roty", 0.)?,
                    ));
                }
            }
        }
        let mut cabin = String::new();
        let mut paths = String::new();
        let mut points = Vec::<Vec3>::new();
        if cfg.contains_key("posdriverhumanoid") {
            append_place(
                &mut cabin,
                "drivpos",
                Vec3::from_array(position(cfg, "posdriverhumanoid")?),
                true,
                0.,
            );
        }
        for (p, sit, rot) in &self.places {
            append_place(&mut cabin, "passpos", *p, *sit, *rot);
            if *sit {
                report.seats += 1;
            } else {
                report.standing += 1;
            }
        }
        for (physical, door) in info.doors.iter().enumerate() {
            let side = if door.name.starts_with("right") {
                "r"
            } else {
                "l"
            };
            let num = door
                .name
                .trim_start_matches("rightdoor")
                .trim_start_matches("leftdoor");
            let key = format!("{side}{num}");
            if !door.from_model {
                let file = info
                    .components
                    .iter()
                    .find(|c| c.role == door.name)
                    .unwrap();
                let d = crate::ini(&file.file)?;
                for (prefix, group) in [("pib", 'a'), ("pia", 'b'), ("pef", 'z'), ("peb", 'y')] {
                    for i in 1..=512 {
                        let s = format!("{prefix}{i}");
                        if !d.contains_key(&s) {
                            break;
                        }
                        self.routes
                            .entry((key.clone(), group))
                            .or_default()
                            .insert(i, Vec3::from_array(position(&d, &s)?));
                    }
                }
            }
            let mut entry = None;
            let mut exit = None;
            for group in ['a', 'b', 'z', 'y'] {
                let Some(route) = self.routes.get(&(key.clone(), group)) else {
                    continue;
                };
                let mut last = None;
                let mut expected = 1;
                for (&n, &p) in route {
                    let id = point(&mut points, &mut paths, p);
                    if n == expected {
                        if let Some(prev) = last {
                            paths.push_str(&format!("[pathlink]\n{prev}\n{id}\n"));
                        }
                    } else {
                        report.warnings.push(format!(
                            "{} route {group}: gap before point {n}; no link invented",
                            door.name
                        ));
                    }
                    if n == 1 && matches!(group, 'a' | 'b') && entry.is_none() {
                        entry = Some(id);
                    }
                    if matches!(group, 'z' | 'y') {
                        exit = Some(id);
                    }
                    last = Some(id);
                    expected = n + 1;
                }
            }
            // Without authored routes, omit the portal: no guessed aisle through seats.
            if door.entrance {
                if let Some(id) = entry {
                    cabin.push_str(&format!("[entry]\n{id}\n{{noticketsale}}\n"));
                    self.entries.push(physical);
                } else {
                    report
                        .warnings
                        .push(format!("{} has no boarding path; entry omitted", door.name));
                }
            }
            if door.exit {
                if let Some(id) = exit {
                    cabin.push_str(&format!("[exit]\n{id}\n"));
                    self.exits.push(physical);
                } else {
                    report
                        .warnings
                        .push(format!("{} has no exit path; exit omitted", door.name));
                }
            }
        }
        ensure!(points.len() <= 8192, "passenger paths exceed point budget");
        report.warnings.push("Passenger routes use authored contiguous points and shared coincident nodes. Turnstile payment, teleport/release flags and per-place before-turnstile behavior are approximated by native boarding; no guessed aisle links are generated".into());
        Ok((cabin, paths))
    }
}
fn point(points: &mut Vec<Vec3>, out: &mut String, p: Vec3) -> usize {
    if let Some(i) = points.iter().position(|v| v.distance(p) < 0.02) {
        return i;
    }
    let i = points.len();
    points.push(p);
    out.push_str(&format!("[pathpnt]\n{}\n{}\n{}\n", p.x, p.y, p.z));
    i
}
fn append_place(out: &mut String, key: &str, p: Vec3, sitting: bool, rot: f32) {
    out.push_str(&format!(
        "[{key}]\n{}\n{}\n{}\n{}\n{rot}\n\n",
        p.x,
        p.y,
        p.z,
        if sitting { 0.45 } else { 0. }
    ));
}
pub(crate) fn center(o: &Object) -> Result<Vec3> {
    ensure!(!o.positions.is_empty(), "empty vehicle marker {}", o.name);
    let (lo, hi) = o.positions.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let p = (lo + hi) * 0.5;
    ensure!(
        p.is_finite() && p.abs().max_element() < 100.,
        "invalid vehicle marker position"
    );
    Ok(p)
}
fn path_tag(name: &str) -> Option<(String, char, usize)> {
    for token in name.split('_') {
        let b = token.as_bytes();
        if b.len() < 4
            || !matches!(b[0], b'r' | b'l')
            || !matches!(b[1], b'1'..=b'6')
            || !matches!(b[2], b'a' | b'b' | b'z' | b'y')
        {
            continue;
        }
        if let Ok(n) = token[3..].parse::<usize>() {
            if n > 0 {
                return Some((token[..2].into(), b[2] as char, n));
            }
        }
    }
    None
}

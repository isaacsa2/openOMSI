//! Export optional gameplay as ordinary, tile-local OMSI scenery and paths.
use crate::{
    convert::EDGE,
    paths::{DrivePath, Point},
    signals::Collector,
    stops::Stop,
    Report,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
};

#[derive(Clone)]
struct Segment {
    a: Vec3,
    b: Vec3,
    rule: Point,
    pedestrian: bool,
    path: usize,
    key: (i32, i32),
    signal: Option<(usize, usize)>,
}

pub(crate) struct Paths {
    segments: Vec<Segment>,
    cells: BTreeMap<(i32, i32), Vec<usize>>,
    probes: usize,
}

impl Paths {
    pub fn new(paths: &[DrivePath]) -> Result<Self> {
        let segments = segments(paths)?;
        let mut cells = BTreeMap::<_, Vec<_>>::new();
        for (i, s) in segments.iter().enumerate().filter(|(_, s)| !s.pedestrian) {
            cells.entry(s.key).or_default().push(i);
        }
        Ok(Self {
            segments,
            cells,
            probes: 0,
        })
    }

    pub fn nearest(&mut self, p: Vec3) -> Result<Option<(f32, f32)>> {
        let (x, y) = key(p);
        let mut best: Option<(f32, f32, f32)> = None;
        for tx in x - 1..=x + 1 {
            for ty in y - 1..=y + 1 {
                for &i in self.cells.get(&(tx, ty)).into_iter().flatten() {
                    self.probes += 1;
                    ensure!(
                        self.probes <= 20_000_000,
                        "stop path matching exceeds import budget"
                    );
                    let s = &self.segments[i];
                    let ab = s.b - s.a;
                    let t = ((p - s.a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
                    let q = s.a + ab * t;
                    let distance = (p - q).length();
                    if distance < 25.0 && best.is_none_or(|old| distance < old.0) {
                        best = Some((distance, ab.x.atan2(ab.y).to_degrees(), q.z));
                    }
                }
            }
        }
        Ok(best.map(|(_, heading, z)| (heading, z)))
    }
}

fn key(p: Vec3) -> (i32, i32) {
    ((p.x / EDGE).floor() as i32, (p.y / EDGE).floor() as i32)
}

fn segments(paths: &[DrivePath]) -> Result<Vec<Segment>> {
    let mut out = Vec::new();
    for (path, definition) in paths.iter().enumerate() {
        for pair in definition.points.windows(2) {
            let a = Vec3::from(pair[0].pos);
            let b = Vec3::from(pair[1].pos);
            if (b - a).truncate().length() < 0.01 {
                continue;
            }
            let mut cuts = vec![0.0, 1.0];
            for axis in 0..2 {
                let lo = (a[axis].min(b[axis]) / EDGE).floor() as i32;
                let hi = (a[axis].max(b[axis]) / EDGE).floor() as i32;
                ensure!(hi - lo <= 4096, "gameplay segment spans too many tiles");
                for edge in lo + 1..=hi {
                    let t = (edge as f32 * EDGE - a[axis]) / (b[axis] - a[axis]);
                    if t > 0.0 && t < 1.0 {
                        cuts.push(t);
                    }
                }
            }
            cuts.sort_by(f32::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
            for cut in cuts.windows(2) {
                let from = a.lerp(b, cut[0]);
                let to = a.lerp(b, cut[1]);
                if (to - from).truncate().length() < 0.01 {
                    continue;
                }
                ensure!(out.len() < 500_000, "native gameplay paths exceed budget");
                out.push(Segment {
                    a: from,
                    b: to,
                    rule: pair[0].clone(),
                    pedestrian: definition.pedestrian,
                    path,
                    key: key(from.lerp(to, 0.5)),
                    signal: None,
                });
            }
        }
    }
    Ok(out)
}

fn attach_signals(
    segments: Vec<Segment>,
    signals: &Collector,
    report: &mut Report,
) -> Result<Vec<Segment>> {
    let mut spatial = BTreeMap::<(i32, i32), Vec<usize>>::new();
    for (i, s) in segments.iter().enumerate().filter(|(_, s)| !s.pedestrian) {
        spatial.entry(s.key).or_default().push(i);
    }
    let mut cuts = BTreeMap::<usize, Vec<(f32, (usize, usize))>>::new();
    let mut probes = 0usize;
    for trigger in &signals.triggers {
        let (x, y) = key(trigger.centre);
        let mut candidates = Vec::new();
        for tx in x - 1..=x + 1 {
            for ty in y - 1..=y + 1 {
                for &i in spatial.get(&(tx, ty)).into_iter().flatten() {
                    probes += 1;
                    ensure!(
                        probes <= 20_000_000,
                        "traffic-light path matching exceeds import budget"
                    );
                    let s = &segments[i];
                    let ab = s.b - s.a;
                    let t = ((trigger.centre - s.a).truncate().dot(ab.truncate())
                        / ab.truncate().length_squared())
                    .clamp(0.0, 1.0);
                    let p = s.a + ab * t;
                    let d = (p - trigger.centre).truncate().length();
                    if d <= trigger.radius
                        && (p.z - trigger.centre.z).abs() <= trigger.height
                        && t < 0.99999
                    {
                        candidates.push((d, i, t));
                    }
                }
            }
        }
        candidates.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.2.total_cmp(&b.2))
                .then(a.1.cmp(&b.1))
        });
        if candidates.is_empty() {
            report.warnings.push("A traffic-light trigger has no matching authored vehicle path; stopping omitted there".into());
            continue;
        }
        if !trigger.any_angle {
            let first = candidates[0];
            if candidates
                .iter()
                .skip(1)
                .any(|c| c.0 - first.0 < 0.05 && segments[c.1].path != segments[first.1].path)
            {
                report.warnings.push("An ambiguous traffic-light trigger overlaps multiple vehicle paths; stopping omitted there".into());
                continue;
            }
            candidates.truncate(1);
        }
        for (_, i, t) in candidates {
            cuts.entry(i)
                .or_default()
                .push((t, (trigger.group, trigger.path * 4 + 3)));
        }
    }
    let mut out = Vec::new();
    for (i, s) in segments.into_iter().enumerate() {
        let Some(mut split) = cuts.remove(&i) else {
            out.push(s);
            continue;
        };
        split.sort_by(|a, b| a.0.total_cmp(&b.0));
        split.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6 && a.1 == b.1);
        let conflicts: Vec<_> = split
            .windows(2)
            .filter(|pair| (pair[0].0 - pair[1].0).abs() < 1e-6 && pair[0].1 != pair[1].1)
            .map(|pair| pair[0].0)
            .collect();
        if !conflicts.is_empty() {
            report.warnings.push(
                "Conflicting traffic-light triggers share a stop line; stopping omitted there"
                    .into(),
            );
            split.retain(|point| !conflicts.iter().any(|t| (point.0 - t).abs() < 1e-6));
        }
        ensure!(
            split.len() <= 64,
            "too many signals on one gameplay segment"
        );
        let mut from = 0.0;
        let mut active = None;
        for (t, signal) in split
            .into_iter()
            .chain(std::iter::once((1.0, (usize::MAX, 0))))
        {
            let a = s.a.lerp(s.b, from);
            let b = s.a.lerp(s.b, t);
            if (b - a).truncate().length() > 0.01 {
                out.push(Segment {
                    a,
                    b,
                    signal: active,
                    ..s.clone()
                });
            }
            from = t;
            active = Some(signal);
        }
    }
    ensure!(out.len() <= 750_000, "signal paths exceed budget");
    Ok(out)
}

fn placement(
    installed: &str,
    stem: &str,
    id: usize,
    pos: Vec3,
    heading: f32,
    labels: &[String],
) -> String {
    let (x, y) = key(pos);
    let mut out = format!("[object]\n0\n{installed}/proton-import/{stem}.sco\n{id}\n{}\n{}\n{}\n{heading}\n0\n0\n{}\n",pos.x-x as f32*EDGE,pos.y-y as f32*EDGE,pos.z,labels.len());
    for label in labels {
        out.push_str(label);
        out.push('\n');
    }
    out
}

fn append_map(dir: &Path, key: (i32, i32), addition: &str) -> Result<()> {
    let file = dir.join(format!("tile_{}_{}.map", key.0, key.1));
    // Tile files already have a UTF-16 LE BOM. Append compatible records directly:
    // re-reading and rewriting a tile per waiting place made large terminals quadratic.
    let mut file = std::fs::OpenOptions::new().append(true).open(file)?;
    let mut bytes = Vec::with_capacity(addition.len() * 2);
    for c in addition.replace('\n', "\r\n").encode_utf16() {
        bytes.extend(c.to_le_bytes());
    }
    file.write_all(&bytes)?;
    Ok(())
}

pub(crate) fn export(
    dir: &Path,
    installed: &str,
    keys: &[(i32, i32)],
    paths: Paths,
    stops: Vec<Stop>,
    mut signals: Collector,
    report: &mut Report,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let valid: BTreeSet<_> = keys.iter().copied().collect();
    let Paths {
        segments, cells, ..
    } = paths;
    drop(cells);
    let segments = attach_signals(segments, &signals, report)?;
    let mut groups = BTreeMap::<((i32, i32), Option<usize>), Vec<Segment>>::new();
    let mut omitted = 0;
    for s in segments {
        if !valid.contains(&s.key) {
            omitted += 1;
            continue;
        }
        if let Some((group, _)) = s.signal {
            signals.programs[group].tiles.insert(s.key);
        }
        groups
            .entry((s.key, s.signal.map(|s| s.0)))
            .or_default()
            .push(s);
    }
    if omitted > 0 {
        report.warnings.push(format!(
            "{omitted} gameplay segments outside occupied scenery tiles omitted"
        ));
    }
    for (group, p) in signals.programs.iter().enumerate() {
        for &key in &p.tiles {
            if valid.contains(&key) {
                groups.entry((key, Some(group))).or_default();
            }
        }
    }
    let mut controllers = BTreeMap::new();
    let mut id = 1_000_000usize;
    for ((key, group), segments) in groups {
        ensure!(
            segments.len() <= 65_535,
            "too many native paths on one tile controller"
        );
        ensure!(!cancel(), "map import cancelled");
        let stem = format!("paths_{}_{}_{}", key.0, key.1, group.map_or(0, |g| g + 1));
        let mut sco = "[absheight]\n[fixed]\n".to_owned();
        if let Some(group) = group {
            let p = &signals.programs[group];
            sco.push_str(&format!(
                "[trafficlight]\n[traffic_lights_group]\n{}\n",
                p.cycle
            ));
            for (i, phases) in p.phases.iter().enumerate() {
                sco.push_str(&format!("[traffic_light]\nchannel_{i}\n"));
                for (state, duration) in phases {
                    sco.push_str(&format!("[phase]\n{state}\n{duration}\n"));
                }
            }
            controllers.insert((key, group), id);
        }
        let mut rules = String::new();
        for (index, s) in segments.iter().enumerate() {
            let a = s.a - Vec3::new(key.0 as f32 * EDGE, key.1 as f32 * EDGE, 0.0);
            let ab = s.b - s.a;
            let length = ab.truncate().length();
            let heading = ab.x.atan2(ab.y).to_degrees();
            let gradient = ab.z / length;
            sco.push_str(&format!("[path]\n{}\n{}\n{}\n{heading}\n0\n{length}\n{gradient}\n{gradient}\n{}\n{}\n0\n{}\n",a.x,a.y,a.z,if s.pedestrian {1} else {0},if s.pedestrian {1.5} else {3.5},match s.rule.turn {1=>2,2=>3,_=>0}));
            if let Some((_, channel)) = s.signal {
                sco.push_str(&format!("[use_traffic_light]\n{channel}\n"));
                report.signal_paths_imported += 1;
            }
            if s.pedestrian {
                report.pedestrian_segments_imported += 1;
            } else {
                report.traffic_segments_imported += 1;
                rules.push_str(&format!(
                    "[rule]\n{index}\ntrafficdensity\n{}\n0\n[rule]\n{index}\nspeedlimit\n{}\n0\n",
                    if s.signal.is_some() {
                        0.0
                    } else {
                        s.rule.density
                    },
                    s.rule.speed
                ));
                if !s.rule.cars {
                    rules.push_str(&format!("[rule]\n{index}\nno_cars\n1\n0\n"));
                }
                if s.rule.buses {
                    rules.push_str(&format!("[rule]\n{index}\nbus\n1\n0\n"));
                }
            }
        }
        std::fs::write(dir.join(format!("proton-import/{stem}.sco")), sco)?;
        let pos = Vec3::new(key.0 as f32 * EDGE, key.1 as f32 * EDGE, 0.0);
        append_map(
            dir,
            key,
            &(placement(installed, &stem, id, pos, 0.0, &[]) + &rules),
        )?;
        id += 1;
    }
    report.signals_imported = controllers
        .keys()
        .map(|(_, group)| *group)
        .collect::<BTreeSet<_>>()
        .len();
    for bulb in signals.bulbs {
        let Some(&parent) = controllers.get(&(bulb.key, bulb.group)) else {
            continue;
        };
        let mut record = placement(
            installed,
            &bulb.stem,
            id,
            Vec3::from(bulb.pos),
            0.0,
            &[bulb.channel.to_string()],
        );
        record.push_str(&format!("[varparent]\n{parent}\n"));
        append_map(dir, bulb.key, &record)?;
        id += 1;
    }
    if report.traffic_segments_imported > 0 {
        std::fs::write(dir.join("proton-import/traffic.enabled"), [])?;
    }
    if !stops.is_empty() {
        std::fs::write(
            dir.join("proton-import/wait.sco"),
            "[absheight]\n[fixed]\n[passengercabin]\nwait.cfg\n",
        )?;
        std::fs::write(
            dir.join("proton-import/wait.cfg"),
            "[passpos]\n0\n0\n0\n0\n0\n",
        )?;
        std::fs::write(
            dir.join("proton-import/stop.sco"),
            "[absheight]\n[fixed]\n[busstop]\n",
        )?;
    }
    for mut stop in stops {
        ensure!(!cancel(), "map import cancelled");
        let pos = Vec3::from(stop.pos.unwrap());
        let owner = key(pos);
        if !valid.contains(&owner) {
            report
                .warnings
                .push(format!("{}: stop outside scenery omitted", stop.name));
            continue;
        }
        if stop.waiting.is_empty() && stop.passengers > 0 {
            report.warnings.push(format!(
                "{}: no waiting markers; native waiting positions approximated",
                stop.name
            ));
            let h = stop.heading.to_radians();
            for i in 0..stop.passengers.min(32) {
                let x = if stop.left { -3.0 } else { 3.0 }
                    + (i % 4) as f32 * 0.6 * if stop.left { -1.0 } else { 1.0 };
                let y = -((i / 4) as f32) * 0.7;
                let p = pos + Vec3::new(x * h.cos() + y * h.sin(), -x * h.sin() + y * h.cos(), 0.0);
                stop.waiting.insert(i, (p.to_array(), stop.heading));
            }
        }
        let amount = stop.passengers.min(stop.waiting.len());
        let labels = vec![
            stop.name,
            amount.to_string(),
            amount.to_string(),
            "10".into(),
            stop.length.to_string(),
            if stop.left { "1" } else { "0" }.into(),
        ];
        append_map(
            dir,
            owner,
            &placement(installed, "stop", id, pos, stop.heading, &labels),
        )?;
        id += 1;
        report.stops_imported += 1;
        for (_, (p, heading)) in stop.waiting {
            let p = Vec3::from(p);
            let key = key(p);
            if valid.contains(&key) {
                append_map(dir, key, &placement(installed, "wait", id, p, heading, &[]))?;
                id += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_split_on_tile_edges_without_losing_slope_or_reverse_direction() {
        let point = |pos| Point {
            pos,
            speed: 40.0,
            turn: 2,
            buses: false,
            cars: true,
            density: 1.0,
        };
        let paths = vec![DrivePath {
            points: vec![point([610.0, 0.0, 10.0]), point([-10.0, 0.0, -10.0])],
            pedestrian: false,
        }];
        let s = segments(&paths).unwrap();
        assert_eq!(
            s.iter().map(|s| s.key).collect::<Vec<_>>(),
            vec![(2, 0), (1, 0), (0, 0), (-1, 0)]
        );
        assert_eq!(s.first().unwrap().a, Vec3::new(610.0, 0.0, 10.0));
        assert_eq!(s.last().unwrap().b, Vec3::new(-10.0, 0.0, -10.0));
        for pair in s.windows(2) {
            assert_eq!(pair[0].b, pair[1].a);
        }
    }

    #[test]
    fn conflicting_stop_triggers_never_choose_an_arbitrary_program() {
        let point = |pos| Point {
            pos,
            speed: 40.0,
            turn: 0,
            buses: false,
            cars: true,
            density: 1.0,
        };
        let paths = vec![DrivePath {
            points: vec![point([0.0, 0.0, 0.0]), point([10.0, 0.0, 0.0])],
            pedestrian: false,
        }];
        let mut signals = Collector::default();
        for group in [0, 1] {
            signals.triggers.push(crate::signals::Trigger {
                group,
                path: 0,
                centre: Vec3::new(5.0, 0.0, 0.0),
                radius: 1.0,
                height: 5.0,
                any_angle: false,
            });
        }
        let mut report = Report::default();
        let lanes = attach_signals(segments(&paths).unwrap(), &signals, &mut report).unwrap();
        assert!(lanes.iter().all(|lane| lane.signal.is_none()));
        assert_eq!(lanes.len(), 1);
        assert!(report.warnings.iter().any(|w| w.contains("Conflicting")));
    }
}

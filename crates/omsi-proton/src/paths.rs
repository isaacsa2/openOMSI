//! Authored GPS strips and vehicle-path points used only by the navigator.
use crate::{
    child, files, ini,
    navigation::{Navigation, Road, Route, MAX_POINTS, MAX_TRIANGLES},
    three_ds::Object,
    Report, Spawn,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use std::{collections::BTreeMap, path::Path};

struct PathDef {
    name: String,
    prefix: Option<String>,
    points: BTreeMap<usize, [f32; 3]>,
    max: usize,
    reverse: bool,
    looped: bool,
    enabled: bool,
}

pub(crate) struct Collector {
    pub data: Navigation,
    paths: Vec<PathDef>,
    triangles: usize,
    points: usize,
}

pub(crate) fn centre(object: &Object) -> Option<Vec3> {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for &p in &object.positions {
        if !p.is_finite() || p.abs().max_element() >= 1_000_000.0 {
            return None;
        }
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (!object.positions.is_empty()).then(|| (lo + hi) * 0.5)
}

impl Collector {
    pub fn new(models: &Path, spawns: &[Spawn]) -> Result<Self> {
        let mut paths = Vec::new();
        let folder = child(models, "aivehicles")?;
        if folder.is_dir() {
            for file in files(&folder, "txt")? {
                let cfg = ini(&file)?;
                let get = |s: &str, k: &str| cfg.get(s).and_then(|s| s.get(k)).map(String::as_str);
                let from_3d = get("from_3d", "readfrom3d") == Some("1");
                let prefix = if from_3d {
                    Some(
                        get("from_3d", "prefix")
                            .filter(|p| !p.is_empty())
                            .ok_or_else(|| anyhow::anyhow!("missing vehicle path prefix"))?
                            .to_owned(),
                    )
                } else {
                    None
                };
                let mut points = BTreeMap::new();
                if !from_3d {
                    for (section, fields) in &cfg {
                        let Ok(index) = section.parse::<usize>() else {
                            continue;
                        };
                        let n = |k: &str| -> Result<f32> {
                            let value = fields
                                .get(k)
                                .map(|v| v.parse::<f32>())
                                .transpose()?
                                .unwrap_or(0.0);
                            ensure!(
                                value.is_finite() && value.abs() < 1_000_000.0,
                                "invalid vehicle path coordinate"
                            );
                            Ok(value)
                        };
                        points.insert(index, [n("posx")?, n("posz")?, n("posy")?]);
                    }
                }
                paths.push(PathDef {
                    name: file
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    prefix,
                    points,
                    max: get("from_3d", "maxpathstocheck")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(999)
                        .min(999),
                    reverse: get("automatic_setup", "reverse") == Some("1"),
                    looped: get("automatic_setup", "loop") == Some("1"),
                    enabled: get("automatic_setup", "enabled") != Some("0"),
                });
            }
        }
        let points = paths.iter().map(|p| p.points.len()).sum();
        ensure!(points <= MAX_POINTS, "too many vehicle path points");
        ensure!(spawns.len() <= 4096, "too many navigation routes");
        Ok(Self {
            data: Navigation {
                version: 1,
                routes: spawns
                    .iter()
                    .map(|s| Route {
                        name: s.name.clone(),
                        triangles: Vec::new(),
                    })
                    .collect(),
                roads: Vec::new(),
            },
            paths,
            triangles: 0,
            points,
        })
    }

    pub fn gps(&mut self, object: &Object, file: &str, report: &mut Report) -> Result<()> {
        let matched: Vec<usize> = self
            .data
            .routes
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let tag = format!("_{}_", r.name);
                (object.name.contains(&tag) || file.contains(&tag)).then_some(i)
            })
            .collect();
        // A route-less strip is unambiguous only in a map with one entry point.
        let matched = if matched.is_empty()
            && self.data.routes.len() == 1
            && !object.name.contains("_gps_")
        {
            vec![0]
        } else {
            matched
        };
        if matched.is_empty() {
            report.gps_skipped += 1;
            return Ok(());
        }
        ensure!(
            self.triangles + object.faces.len() * matched.len() <= MAX_TRIANGLES,
            "GPS geometry exceeds navigation budget"
        );
        for face in &object.faces {
            let vertices = face.map(|i| object.positions.get(i as usize).copied());
            let [Some(a), Some(b), Some(c)] = vertices else {
                anyhow::bail!("invalid GPS face index")
            };
            ensure!(
                [a, b, c]
                    .iter()
                    .all(|p| p.is_finite() && p.abs().max_element() < 1_000_000.0),
                "invalid GPS coordinate"
            );
            let tri = [
                a.truncate().to_array(),
                b.truncate().to_array(),
                c.truncate().to_array(),
            ];
            for &i in &matched {
                self.data.routes[i].triangles.push(tri);
                self.triangles += 1;
            }
        }
        report.gps_imported += 1;
        Ok(())
    }

    pub fn marker(&mut self, object: &Object) -> Result<bool> {
        let mut found = false;
        for path in &mut self.paths {
            let Some(prefix) = &path.prefix else { continue };
            let Some(suffix) = object
                .name
                .strip_prefix(prefix)
                .and_then(|s| s.strip_prefix('.'))
            else {
                continue;
            };
            if suffix.len() != 3 || !suffix.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            found = true;
            let index: usize = suffix.parse()?;
            if index > path.max {
                continue;
            }
            let p = centre(object)
                .ok_or_else(|| anyhow::anyhow!("invalid road marker centre"))?
                .to_array();
            if let Some(old) = path.points.insert(index, p) {
                ensure!(old == p, "ambiguous road marker {}", object.name);
            } else {
                self.points += 1;
            }
            ensure!(self.points <= MAX_POINTS, "too many vehicle path points");
        }
        Ok(found)
    }

    pub fn finish(mut self, report: &mut Report) -> Result<Navigation> {
        for path in self.paths {
            if !path.enabled {
                report.warnings.push(format!(
                    "{}: manual vehicle-path links are not imported into the navigator",
                    path.name
                ));
                continue;
            }
            let mut runs: Vec<Vec<[f32; 3]>> = vec![Vec::new()];
            let mut previous = None;
            for (index, point) in path.points {
                if previous.is_some_and(|p| index != p + 1) {
                    runs.push(Vec::new());
                }
                let run = runs.last_mut().unwrap();
                if run.last() != Some(&point) {
                    run.push(point);
                }
                previous = Some(index);
            }
            if runs.len() > 1 {
                report.warnings.push(format!(
                    "{}: gaps in road markers retained as disconnected paths",
                    path.name
                ));
            }
            let close = path.looped && runs.len() == 1;
            for mut points in runs.into_iter().filter(|p| p.len() >= 2) {
                if path.reverse {
                    points.reverse();
                }
                if close && points.first() != points.last() {
                    points.push(points[0]);
                }
                self.data.roads.push(Road {
                    name: path.name.clone(),
                    points,
                });
            }
        }
        report.roads_imported = self.data.roads.len();
        if report.gps_skipped > 0 {
            report.warnings.push(format!(
                "{} GPS helpers could not be matched to an entry-point name and were omitted",
                report.gps_skipped
            ));
        }
        ensure!(self.data.roads.len() <= 4096, "too many navigation roads");
        ensure!(
            self.data
                .roads
                .iter()
                .map(|r| r.points.len())
                .sum::<usize>()
                <= MAX_POINTS,
            "road loops exceed navigation budget"
        );
        Ok(self.data)
    }
}

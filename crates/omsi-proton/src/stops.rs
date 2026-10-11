//! Stops retain only their trigger bounds and authored waiting positions.
use crate::{
    child, files, ini,
    paths::centre,
    settings::{label, number, position, value},
    three_ds::Object,
    Report,
};
use anyhow::{ensure, Result};
use glam::Vec3;
use std::{collections::BTreeMap, path::Path};

pub(crate) struct Stop {
    pub name: String,
    prefix: Option<String>,
    pub pos: Option<[f32; 3]>,
    pub heading: f32,
    pub left: bool,
    pub passengers: usize,
    pub length: f32,
    pub waiting: BTreeMap<usize, ([f32; 3], f32)>,
    max: usize,
    from_3d: bool,
}

#[derive(Default)]
pub(crate) struct Collector {
    pub stops: Vec<Stop>,
    points: usize,
}

impl Collector {
    pub fn new(models: &Path) -> Result<Self> {
        let folder = child(models, "busstops")?;
        let mut out = Self::default();
        if !folder.is_dir() {
            return Ok(out);
        }
        for file in files(&folder, "txt")? {
            ensure!(out.stops.len() < 4096, "too many bus stops");
            let cfg = ini(&file)?;
            let from_3d = value(&cfg, "from_3d", "readfrom3d") == Some("1");
            let mut stop = Stop {
                name: label(
                    value(&cfg, "busstop", "name")
                        .unwrap_or(&file.file_stem().unwrap().to_string_lossy()),
                )?,
                prefix: if from_3d {
                    value(&cfg, "from_3d", "prefix").map(label).transpose()?
                } else {
                    None
                },
                pos: if from_3d {
                    None
                } else {
                    Some(position(&cfg, "busstop")?)
                },
                heading: number(
                    &cfg,
                    "busstop",
                    "roty",
                    number(&cfg, "from_3d", "defpaxroty", 0.0)?,
                )?
                .rem_euclid(360.0),
                left: value(&cfg, "busstop", "isleft") == Some("1"),
                passengers: number(&cfg, "busstop", "paxamount", 10.0)?.clamp(0.0, 256.0) as usize,
                length: (number(&cfg, "busstop", "radius", 1.0)?.clamp(0.1, 100.0) * 2.0 + 15.0)
                    .max(30.0),
                waiting: BTreeMap::new(),
                max: number(&cfg, "from_3d", "maxpathstocheck", 999.0)?.clamp(0.0, 999.0) as usize,
                from_3d,
            };
            if !from_3d {
                for section in cfg.keys().filter(|s| s.starts_with("standing_")) {
                    let index = section.trim_start_matches("standing_").parse::<usize>()?;
                    ensure!(index <= 999, "too many stop waiting places");
                    let local = Vec3::from(position(&cfg, section)?);
                    let h = stop.heading.to_radians();
                    let rotated = Vec3::new(
                        local.x * h.cos() + local.y * h.sin(),
                        -local.x * h.sin() + local.y * h.cos(),
                        local.z,
                    );
                    let pos = (Vec3::from(stop.pos.unwrap()) + rotated).to_array();
                    stop.waiting.insert(
                        index,
                        (pos, stop.heading + number(&cfg, section, "roty", 0.0)?),
                    );
                }
            }
            // Per-point rotation overrides are kept without holding source meshes.
            for (section, fields) in &cfg {
                if let Ok(index) = section.parse::<usize>() {
                    if index <= stop.max {
                        if let Some(rot) = fields.get("roty").filter(|r| !r.is_empty()) {
                            let rot: f32 = rot.parse()?;
                            ensure!(rot.is_finite(), "invalid stop waiting rotation");
                            stop.waiting.insert(index, ([f32::NAN; 3], rot));
                        }
                    }
                }
            }
            out.points += stop.waiting.len();
            ensure!(out.points <= 100_000, "too many stop waiting places");
            out.stops.push(stop);
        }
        ensure!(out.points <= 100_000, "too many stop waiting places");
        Ok(out)
    }

    pub fn marker(&mut self, object: &Object) -> Result<bool> {
        let mut found = false;
        for stop in &mut self.stops {
            let Some(prefix) = &stop.prefix else { continue };
            if object.name == format!("{prefix}_trigger") {
                let mut p =
                    centre(object).ok_or_else(|| anyhow::anyhow!("invalid stop trigger"))?;
                let lo = object
                    .positions
                    .iter()
                    .map(|p| p.z)
                    .fold(f32::INFINITY, f32::min);
                p.z = lo;
                let pos = p.to_array();
                ensure!(
                    stop.pos.is_none_or(|old| old == pos),
                    "ambiguous stop trigger"
                );
                stop.pos = Some(pos);
                found = true;
                continue;
            }
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
            if index > stop.max {
                continue;
            }
            let p = centre(object)
                .ok_or_else(|| anyhow::anyhow!("invalid waiting marker"))?
                .to_array();
            let rot = stop.waiting.get(&index).map_or(stop.heading, |(_, r)| *r);
            if let Some((old, _)) = stop.waiting.insert(index, (p, rot)) {
                ensure!(
                    old.iter().any(|p| p.is_nan()) || old == p,
                    "ambiguous waiting marker"
                );
            } else {
                self.points += 1;
            }
            ensure!(self.points <= 100_000, "too many stop waiting places");
        }
        Ok(found)
    }

    pub fn finish(
        self,
        paths: &mut crate::gameplay::Paths,
        report: &mut Report,
    ) -> Result<Vec<Stop>> {
        let mut out = Vec::new();
        for mut stop in self.stops {
            let Some(pos) = stop.pos else {
                report.warnings.push(format!(
                    "{}: missing bus-stop trigger; stop omitted",
                    stop.name
                ));
                continue;
            };
            stop.waiting
                .retain(|_, (p, _)| p.iter().all(|v| v.is_finite()));
            // A 3D trigger carries position, not direction. Use the closest authored
            // vehicle path, if present, rather than inventing a bus line or trip.
            if stop.from_3d {
                let p = Vec3::from(pos);
                if let Some((heading, z)) = paths.nearest(p)? {
                    stop.heading = heading.rem_euclid(360.0);
                    if (z - pos[2]).abs() < 5.0 {
                        stop.pos.as_mut().unwrap()[2] = z;
                    }
                }
            }
            out.push(stop);
        }
        Ok(out)
    }
}

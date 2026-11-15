//! Some partial map exports keep an entry point from a different route. Keep usable
//! authored starts, but report and relocate a start with no nearby supporting face.
#[cfg(test)]
use crate::convert::Tiles;
use crate::{convert::EDGE, Spawn};
use anyhow::{bail, ensure, Context, Result};
use glam::Vec3;

pub(crate) fn number(
    raw: Option<&String>,
    key: &str,
    section: &str,
    file: &std::path::Path,
    warnings: &mut Vec<String>,
) -> Result<f64> {
    let raw = raw.map(String::as_str).unwrap_or("0");
    if raw.trim().is_empty() && key == "roty" {
        warnings.push(format!(
            "Entry point {section}.{key} in {}: empty rotation interpreted as 0 degrees.",
            file.display()
        ));
        return Ok(0.0);
    }
    // Some authored starts append a degree suffix to rotY. Restrict the repair
    // to the angle: accepting a numeric prefix everywhere would hide damaged data.
    let degrees = (key == "roty")
        .then(|| {
            raw.strip_suffix('d')
                .or_else(|| raw.strip_suffix('D'))
                .or_else(|| raw.strip_suffix('\u{00b0}'))
        })
        .flatten();
    let n = degrees
        .unwrap_or(raw)
        .trim()
        .parse::<f64>()
        .with_context(|| {
            format!(
                "invalid entry point {section}.{key}={raw:?} in {}",
                file.display()
            )
        })?;
    ensure!(
        n.is_finite() && n.abs() < 1_000_000.0,
        "invalid entry point {section}.{key}={raw:?} in {}: non-finite or out of range",
        file.display()
    );
    if degrees.is_some() {
        warnings.push(format!(
            "Entry point {section}.{key}={raw:?} in {}: interpreted the angle as {n} degrees.",
            file.display()
        ));
    }
    Ok(n)
}

fn height(face: [Vec3; 3], x: f32, y: f32) -> Option<f32> {
    let [a, b, c] = face;
    let denominator = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
    if denominator.abs() < 1e-8 {
        return None;
    }
    let u = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / denominator;
    let v = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / denominator;
    (u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001)
        .then_some(u * a.z + v * b.z + (1.0 - u - v) * c.z)
}

struct Check {
    pos: [f64; 3],
    supported: bool,
    nearest: Option<(f64, glam::DVec3)>,
}

pub(crate) struct Probe {
    checks: Vec<Check>,
    by_tile: std::collections::BTreeMap<(i32, i32), Vec<usize>>,
}

impl Probe {
    pub fn new(spawns: &[Spawn]) -> Self {
        let mut by_tile: std::collections::BTreeMap<_, Vec<_>> = std::collections::BTreeMap::new();
        let checks = spawns
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let key = (
                    (s.pos[0] / EDGE as f64).floor() as i32,
                    (s.pos[1] / EDGE as f64).floor() as i32,
                );
                by_tile.entry(key).or_default().push(i);
                Check {
                    pos: s.pos,
                    supported: false,
                    nearest: None,
                }
            })
            .collect();
        Self { checks, by_tile }
    }

    pub fn support(&mut self, key: (i32, i32), face: [Vec3; 3]) {
        let Some(indices) = self.by_tile.get(&key) else {
            return;
        };
        for &i in indices {
            let c = &mut self.checks[i];
            if c.supported {
                continue;
            }
            let [x, y, z] = c.pos;
            let x = x as f32 - key.0 as f32 * EDGE;
            let y = y as f32 - key.1 as f32 * EDGE;
            c.supported = height(face, x, y).is_some_and(|h| (h as f64 - z).abs() <= 3.0);
        }
    }

    pub fn nearest(&mut self, key: (i32, i32), mesh: &omsi_o3d::Mesh) {
        if self.checks.iter().all(|c| c.supported) {
            return;
        }
        let origin = Vec3::new(key.0 as f32 * EDGE, key.1 as f32 * EDGE, 0.0);
        for t in &mesh.triangles {
            let [a, b, c] = t.indices.map(|i| mesh.vertices[i as usize].position);
            let normal = (b - a).cross(c - a);
            if normal.length() < 4.0 || normal.normalize_or_zero().z.abs() < 0.9 {
                continue;
            }
            let center = ((a + b + c) / 3.0 + origin).as_dvec3();
            for check in self.checks.iter_mut().filter(|c| !c.supported) {
                let [x, y, z] = check.pos;
                let distance =
                    (center.x - x).powi(2) + (center.y - y).powi(2) + (center.z - z).powi(2);
                if check
                    .nearest
                    .as_ref()
                    .is_none_or(|(best, _)| distance < *best)
                {
                    check.nearest = Some((distance, center));
                }
            }
        }
    }

    pub fn finish(&self, spawns: &mut [Spawn], warnings: &mut Vec<String>) -> Result<()> {
        for (spawn, check) in spawns.iter_mut().zip(&self.checks) {
            if check.supported {
                continue;
            }
            let Some((distance, center)) = check.nearest else {
                bail!("entry point {} has no usable static surface", spawn.name);
            };
            warnings.push(format!("Entry point {} has no nearby surface; moved {:.1} m to a static mesh face. Check its position and clearance in game.", spawn.name, distance.sqrt()));
            spawn.pos = [center.x, center.y, center.z + 0.5];
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate(
    tiles: &Tiles,
    spawns: &mut [Spawn],
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut probe = Probe::new(spawns);
    for (&key, mesh) in tiles {
        for t in &mesh.triangles {
            probe.support(key, t.indices.map(|i| mesh.vertices[i as usize].position));
        }
    }
    for (&key, mesh) in tiles {
        probe.nearest(key, mesh);
    }
    probe.finish(spawns, warnings)
}

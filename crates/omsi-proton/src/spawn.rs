//! Some partial map exports keep an entry point from a different route. Keep usable
//! authored starts, but report and relocate a start with no nearby supporting face.
use crate::{
    convert::{Tiles, EDGE},
    Spawn,
};
use anyhow::{bail, Result};
use glam::Vec3;

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

fn supported(tiles: &Tiles, spawn: &Spawn) -> bool {
    let [x, y, z] = spawn.pos;
    let key = (
        (x / EDGE as f64).floor() as i32,
        (y / EDGE as f64).floor() as i32,
    );
    let Some(mesh) = tiles.get(&key) else {
        return false;
    };
    let x = x as f32 - key.0 as f32 * EDGE;
    let y = y as f32 - key.1 as f32 * EDGE;
    mesh.triangles.iter().any(|t| {
        let face = t.indices.map(|i| mesh.vertices[i as usize].position);
        height(face, x, y).is_some_and(|h| (h as f64 - z).abs() <= 3.0)
    })
}

pub(crate) fn validate(
    tiles: &Tiles,
    spawns: &mut [Spawn],
    warnings: &mut Vec<String>,
) -> Result<()> {
    for spawn in spawns {
        if supported(tiles, spawn) {
            continue;
        }
        let mut nearest = None;
        for (&(tx, ty), mesh) in tiles {
            let origin = Vec3::new(tx as f32 * EDGE, ty as f32 * EDGE, 0.0);
            for t in &mesh.triangles {
                let [a, b, c] = t.indices.map(|i| mesh.vertices[i as usize].position);
                let normal = (b - a).cross(c - a);
                // A reasonably flat, nondegenerate face; walls are not a spawn surface.
                if normal.length() < 4.0 || normal.normalize_or_zero().z.abs() < 0.9 {
                    continue;
                }
                let center = ((a + b + c) / 3.0 + origin).as_dvec3();
                let [x, y, z] = spawn.pos;
                let distance =
                    (center.x - x).powi(2) + (center.y - y).powi(2) + (center.z - z).powi(2);
                if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
                    nearest = Some((distance, center));
                }
            }
        }
        let Some((distance, center)) = nearest else {
            bail!("entry point {} has no usable static surface", spawn.name);
        };
        warnings.push(format!("Entry point {} has no nearby surface; moved {:.1} m to a static mesh face. Check its position and clearance in game.", spawn.name, distance.sqrt()));
        spawn.pos = [center.x, center.y, center.z + 0.5];
    }
    Ok(())
}

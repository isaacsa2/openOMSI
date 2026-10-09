//! What the ambience asks of the world: the surface under a tyre, and what stands around
//! the listener (trees, houses).

use super::*;

/// Trees farther than this are not heard rustling (m).
const TREE_REACH: f64 = 60.0;

/// Around the listener: how much foliage (0 … 1), where it stands (-1 left … 1 right of
/// `right`), how built-up the place is (0 … 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Surroundings {
    pub foliage: f32,
    pub balance: f32,
    pub urban: f32,
}

impl World {
    /// OMSI's `[surface]` id under a tyre whose contact is at `at` (see
    /// `TileSurface::surface_under`); `None` where no tile is loaded.
    pub fn surface_under(&self, at: DVec3) -> Option<u8> {
        let key = tile_key(at.x, at.y);
        let (lx, ly) = ((at.x - key.0 as f64 * tile_size()) as f32, (at.y - key.1 as f64 * tile_size()) as f32);
        let terrain = self.terrains.read().get(&key).map(|t| t.sample(lx, ly));
        let surfaces = self.surfaces.read();
        let s = surfaces.get(&key)?;
        Some(s.surface_under(lx, ly, at.z as f32, terrain))
    }

    /// The trees and houses around `at`: each tree within reach counts with its crown
    /// (height²) over its distance² - the sound of a crown falls off like any source's - so a
    /// park around the listener is near 1 and a single tree across the road a little; the
    /// place is built-up by the objects on the tiles around.
    pub fn surroundings(&self, at: DVec3, right: glam::Vec3) -> Surroundings {
        let key = tile_key(at.x, at.y);
        let surfaces = self.surfaces.read();
        let (mut sum, mut side, mut objects, mut tiles) = (0.0f64, 0.0f64, 0u32, 0u32);
        let r = right.as_dvec3();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let k = (key.0 + dx, key.1 + dy);
                let Some(s) = surfaces.get(&k) else { continue };
                tiles += 1;
                objects += s.sound.objects;
                let o = DVec3::new(k.0 as f64 * tile_size(), k.1 as f64 * tile_size(), 0.0);
                for t in &s.sound.trees {
                    let d = DVec3::new(o.x + t[0] as f64 - at.x, o.y + t[1] as f64 - at.y, 0.0);
                    let dist2 = d.length_squared();
                    if dist2 > TREE_REACH * TREE_REACH {
                        continue;
                    }
                    // a 10 m tree 10 m away counts 0.1 (a park of a dozen around: 1)
                    let w = (t[2] as f64).clamp(2.0, 30.0).powi(2) / (dist2 + 25.0) * 0.1;
                    sum += w;
                    side += w * d.normalize_or_zero().dot(r);
                }
            }
        }
        let foliage = (1.0 - (-sum).exp()) as f32;
        let balance = if sum > 0.0 { (side / sum) as f32 } else { 0.0 };
        // some 300 objects a tile in a town quarter, a few dozen in the country
        let per_tile = objects as f32 / tiles.max(1) as f32;
        let urban = (per_tile / 400.0).clamp(0.0, 1.0);
        Surroundings { foliage, balance: balance.clamp(-1.0, 1.0), urban }
    }
}

//! Prepare a small, bounded region of real OMSI terrain for the BGFX viewer.
//!
//! This is the existing openOMSI global.cfg + terrain parser and mesh generator.
//! It deliberately does not invent map formats or copy game files.
//!
//! For now this preview draws terrain height-fields only, not full map scenery,
//! splines, textures, traffic or simulation. Geometry is rebased around the
//! selected centre tile to retain f32 precision on large world-coordinate maps.

use std::path::{Path, PathBuf};

use glam::{Mat4, Vec3};

use crate::{geometry_bridge::pack_generated, o3d_bridge::PackedO3d};

#[derive(Debug, Clone)]
pub struct PreviewTile {
    pub coordinates: (i32, i32),
    pub geometry: PackedO3d,
    pub was_missing_heightfield: bool,
}

#[derive(Debug, Clone)]
pub struct MapPreview {
    pub map_name: String,
    pub center: (i32, i32),
    pub tile_size: f32,
    pub tiles: Vec<PreviewTile>,
}

#[derive(Debug, Clone, Copy)]
pub struct Selection {
    /// Explicit centre tile from the map's [map] list, or its first tile.
    pub center: Option<(i32, i32)>,
    /// Maximum tile radius is 2 (a 5x5 square), to cap VRAM and startup.
    pub radius: u8,
}

impl Selection {
    pub fn validate(self) -> Result<Self, String> {
        if self.radius > 2 {
            return Err("map preview radius cannot exceed 2 tiles".into());
        }
        Ok(self)
    }
}

pub fn load_terrain_preview(global_cfg: &Path, selection: Selection) -> Result<MapPreview, String> {
    selection.validate()?;
    let global = omsi_map::GlobalCfg::load(global_cfg)
        .map_err(|e| format!("{}: {e}", global_cfg.display()))?;
    if global.tiles.is_empty() {
        return Err("global.cfg has no map tiles".into());
    }
    // The same world-coordinate geometry defaults as the production loader.
    omsi_map::configure_grid(&global);
    let center = selection.center.unwrap_or((global.tiles[0].x, global.tiles[0].y));
    if !global.tiles.iter().any(|t| (t.x, t.y) == center) {
        return Err(format!("selected tile ({}, {}) is not listed in global.cfg", center.0, center.1));
    }
    let root = global_cfg.parent()
        .ok_or_else(|| "global.cfg has no parent map directory".to_string())?;
    let size = omsi_map::tile_size();
    if !size.is_finite() || size <= 0.0 || size > 100_000.0 {
        return Err("invalid tile size after world-grid configuration".into());
    }
    let r = i64::from(selection.radius);
    let mut tiles = Vec::new();
    for tile in &global.tiles {
        let dx = i64::from(tile.x) - i64::from(center.0);
        let dy = i64::from(tile.y) - i64::from(center.1);
        if dx.abs() > r || dy.abs() > r {
            continue;
        }
        let height_file = PathBuf::from(format!(
            "{}.terrain", root.join(&tile.file).display()
        ));
        // The normal engine uses VFS fallback to flat terrain for maps whose
        // original height field is missing; match that preview behavior.
        let (heightfield, missing) = match omsi_map::Terrain::load(&height_file) {
            Ok(t) => (t, false),
            Err(_) => (omsi_map::Terrain::flat(), true),
        };
        let cpu_mesh = omsi_geometry::build_terrain_mesh(&heightfield);
        let mut geometry = pack_generated(&cpu_mesh)
            .map_err(|e| format!("tile ({}, {}): {e}", tile.x, tile.y))?
            .mesh;
        // Use offset from the preview centre, not huge absolute f32 world
        // coordinates. Keeping the original x/y index separately also makes
        // it possible to reposition future streamed terrain as the camera
        // moves to another tile.
        let pos = Vec3::new((dx as f64 * size) as f32, (dy as f64 * size) as f32, 0.0);
        geometry.model_transform = Mat4::from_translation(pos).to_cols_array();
        tiles.push(PreviewTile {
            coordinates: (tile.x, tile.y),
            geometry,
            was_missing_heightfield: missing,
        });
    }
    if tiles.is_empty() {
        return Err("no terrain tiles inside selected preview radius".into());
    }
    if tiles.len() > 25 {
        return Err("preview exceeded the 25-tile safety limit".into());
    }
    Ok(MapPreview {
        map_name: global.friendly_name.clone(),
        center,
        tile_size: size as f32,
        tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_is_limited_to_25_tiles() {
        assert!(Selection { center: None, radius: 2 }.validate().is_ok());
        assert!(Selection { center: None, radius: 3 }.validate().is_err());
    }
}

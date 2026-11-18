//! CPU mesh placement for static scenery ground surfaces.
//!
//! The first mesh of a [surface] object is part of OMSI's wheel ground.
//! Render-only ground layers are rasterized as terrain overlays but do not
//! become wheel collision. Deformed crossings, cutter-backed surfaces and
//! animated meshes remain explicitly unsupported until CPU deformation and
//! terrain-hole logic is equivalent to the graphical host.

use crate::surfaces::{SurfaceCounts, raster_overlaps_tile};
use crate::world::HostWorld;
use glam::{DVec2, DVec3, Mat4};
use omsi_geometry::{MeshData, TileSurface};
use omsi_map::{tile::Tile, tile_size};
use omsi_model::Model;
use omsi_scenery::SceneryObject;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_SURFACE_TRIANGLES: usize = 80_000;
const MAX_OBJECT_MESH_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
enum SurfaceDef {
    None,
    Unsupported,
    Static { mesh: Arc<MeshData>, drivable: bool },
}

pub(crate) struct ScenerySurfaces {
    definitions: HashMap<PathBuf, SurfaceDef>,
}

impl ScenerySurfaces {
    pub(crate) fn new() -> Self {
        Self { definitions: HashMap::new() }
    }

    fn load(&mut self, def: &SceneryObject) -> SurfaceDef {
        let key = def.path.clone();
        if let Some(previous) = self.definitions.get(&key) {
            return previous.clone();
        }
        if self.definitions.len() >= 256 {
            self.definitions.clear();
        }
        let result = Self::parse(def);
        self.definitions.insert(key, result.clone());
        result
    }

    fn parse(def: &SceneryObject) -> SurfaceDef {
        if def.only_editor || def.tree.is_some() {
            return SurfaceDef::None;
        }
        let parent = def.path.parent().unwrap_or(Path::new(""));
        let (model, directory) = match def.model_file.as_deref() {
            Some(file) => {
                let path = omsi_cfg::resolve_path(parent, file);
                let Ok(model) = Model::load(&path) else {
                    return SurfaceDef::Unsupported;
                };
                let Some(dir) = path.parent() else { return SurfaceDef::Unsupported };
                (model, dir.to_path_buf())
            }
            None => (def.model.clone(), parent.to_path_buf()),
        };
        let mut sco = def.clone();
        sco.inherit_model_tags(&model);
        if !sco.surface && !sco.render_type.is_ground_layer() {
            return SurfaceDef::None;
        }
        // The visual host deforms these meshes and cuts terrain before
        // publishing physics. Their undeformed triangles would be unsafe.
        if sco.crossing_height_deformation.is_some()
            || model.terrain_hole_meshes().next().is_some()
            || sco.model.terrain_hole_meshes().next().is_some()
        {
            return SurfaceDef::Unsupported;
        }
        let first = if model.lods.is_empty() {
            model.meshes.first()
        } else {
            model.lod_meshes(0).first()
        };
        let Some(first) = first else { return SurfaceDef::Unsupported };
        if !first.animations.is_empty() {
            return SurfaceDef::Unsupported;
        }
        let paths = [
            omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&directory, "model"), &first.file),
            omsi_cfg::resolve_path(&directory, &first.file),
        ];
        let Some(path) = paths.into_iter().find(|p| omsi_cfg::vfs::is_file(p)) else {
            return SurfaceDef::Unsupported;
        };
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_OBJECT_MESH_BYTES) {
            return SurfaceDef::Unsupported;
        }
        let Ok(loaded) = omsi_o3d::load_mesh(&path) else {
            log::warn!("headless scenery ground: could not load {}", path.display());
            return SurfaceDef::Unsupported;
        };
        if loaded.triangles.len() > MAX_SURFACE_TRIANGLES
            || loaded.triangles.iter().any(|t| {
                t.indices.iter().any(|&i| i as usize >= loaded.vertices.len())
            })
        {
            return SurfaceDef::Unsupported;
        }
        let mesh = omsi_geometry::mesh_from_o3d(&loaded);
        if mesh.indices.is_empty() {
            return SurfaceDef::Unsupported;
        }
        SurfaceDef::Static { mesh: Arc::new(mesh), drivable: sco.surface }
    }
}

impl HostWorld {
    /// Publish only static, undistorted scenery surfaces into the active
    /// tile rasters. Existing map-index coordinates preserve Chrono and
    /// geographic map placement; no GPU meshes or texture files are loaded.
    pub(crate) fn install_scenery_ground(
        &mut self,
        key: (i32, i32),
        tile: &Tile,
        rasters: &mut HashMap<(i32, i32), TileSurface>,
        cache: &mut ScenerySurfaces,
        counts: &mut SurfaceCounts,
    ) {
        let root = self.data.snapshot().map.root.clone();
        for obj in &tile.objects {
            if obj.file.trim().is_empty() {
                continue;
            }
            let Some(def) = self.scenery(&root, &obj.file) else {
                counts.missing_definitions += 1;
                continue;
            };
            let loaded = cache.load(&def);
            let SurfaceDef::Static { mesh, drivable } = loaded else {
                if matches!(loaded, SurfaceDef::Unsupported) {
                    counts.pending_scenery_surfaces += 1;
                }
                continue;
            };
            if obj.parent_id.is_some() {
                counts.pending_scenery_surfaces += 1;
                continue;
            }
            let indexed = self.index.duplicates.get(&(key, obj.id)).copied()
                .or_else(|| self.index.objects.get(&obj.id)
                    .and_then(|(owner, pos, rot)| (*owner == key).then_some((*pos, *rot))));
            let (pos, raw_rot) = match indexed {
                Some(p) => p,
                None => {
                    let base = DVec2::new(
                        key.0 as f64 * tile_size() + obj.pos[0],
                        key.1 as f64 * tile_size() + obj.pos[1],
                    );
                    let height = if def.absolute_height() { 0.0 } else {
                        self.terrain_height(base.x, base.y).map(f64::from).unwrap_or(0.0)
                    };
                    (DVec3::new(base.x, base.y, obj.pos[2] + height), obj.rot)
                }
            };
            if !pos.is_finite() || !raw_rot.iter().all(|v| v.is_finite()) {
                counts.pending_scenery_surfaces += 1;
                continue;
            }
            let rotation = omsi_geometry::object_rotation(
                omsi_geometry::map_rotation(raw_rot),
            );
            for (&target, surface) in rasters.iter_mut() {
                if !raster_overlaps_tile(&mesh, &rotation, pos, target) {
                    continue;
                }
                surface.rasterize_kind(&mesh, &rotation, pos, target.0, target.1, drivable);
                if drivable {
                    surface.add_drive_mesh(&mesh, &rotation, pos, target.0, target.1);
                }
                counts.placed_scenery_surfaces += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn rotated_surface_geometry_can_reach_a_neighbor() {
        let mesh = MeshData {
            positions: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, 0.0),
                Vec3::new(0.0, 20.0, 0.0),
            ],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let pos = DVec3::new(tile_size() - 5.0, 5.0, 0.0);
        assert!(raster_overlaps_tile(&mesh, &Mat4::IDENTITY, pos, (1, 0)));
        assert!(!raster_overlaps_tile(&mesh, &Mat4::IDENTITY, pos, (2, 0)));
    }
}

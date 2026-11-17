//! CPU-only map loading for the dedicated host.
//!
//! Read global.cfg, Chrono lists and the whole-map index. Neither the world
//! metadata nor its collision/traffic snapshot owns a wgpu device or a scene
//! graph. The 3D renderer is not involved in this operation.

use anyhow::{Context, Result};
use omsi_map::global::GlobalCfg;
use omsi_sim::world_data::{MapFacts, WorldCell, WorldData};
use omsi_world::map_index::MapIndex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// CPU data that stays alive for the lifetime of a dedicated map session.
pub struct HostWorld {
    pub global: GlobalCfg,
    pub map_dir: PathBuf,
    pub data: Arc<WorldCell>,
    pub index: Arc<MapIndex>,
}

impl HostWorld {
    /// `date` is the YYYYMMDD date used for OMSI Chrono selections.
    ///
    /// This phase loads the **map's CPU index and metadata**, not GPU meshes.
    /// Road/collision tile streaming and its sim adapters must be installed by
    /// the actual host engine before accepting players.
    pub fn open(root: &Path, global_path: &Path, date: i32) -> Result<Self> {
        let mut global = GlobalCfg::load(global_path)
            .with_context(|| format!("reading map {}", global_path.display()))?;
        omsi_map::configure_grid(&global);
        if global.entry_points.is_empty() {
            global.add_object_entry_points();
        }
        let map_dir = global.dir().to_path_buf();
        let chrono_dirs = omsi_map::active_chrono_dirs(&map_dir, date);
        let ailists = omsi_map::ailists::ailists_with_chrono(&map_dir, &chrono_dirs);
        let files: Vec<(usize, i32, i32, PathBuf)> = global
            .tiles
            .iter()
            .map(|t| (t.index, t.x, t.y, omsi_cfg::resolve_path(&map_dir, &t.file)))
            .collect();

        // CPU indexing is parallel; the resulting data structures are immutable
        // and shared, with no textures/mesh buffers to upload or retain.
        let index = Arc::new(MapIndex::build(&files, &chrono_dirs, root));
        let mut data = WorldData {
            map: Arc::new(MapFacts {
                root: root.to_path_buf(),
                map_dir: map_dir.clone(),
                raw_tiles: global.raw_tiles.clone(),
                tiles: global.tiles.iter().map(|t| ((t.x, t.y), t.file.clone())).collect(),
                ailists,
                date,
                chrono_dirs,
            }),
            tile_files: Some(Arc::new(
                files
                    .iter()
                    .filter(|(_, _, _, path)| omsi_cfg::vfs::is_file(path))
                    .map(|(_, x, y, _)| (*x, *y))
                    .collect(),
            )),
            ..Default::default()
        };
        let objects = Arc::make_mut(&mut data.objects);
        objects.add_index(
            index.objects.iter().map(|(id, (_tile, pos, rot))| (*id, (*pos, *rot))),
            index.duplicates.iter().map(|(key, (pos, rot))| (*key, (*pos, *rot))),
        );
        data.index = Some(Arc::clone(&index));

        log::info!(
            "dedicated CPU world: {} tiles indexed, {} unreadable, {} placed objects (no renderer allocated)",
            index.tiles_read, index.tiles_failed, index.objects.len()
        );
        Ok(Self {
            global,
            map_dir,
            data: Arc::new(WorldCell::new(data)),
            index,
        })
    }
}

/// The map of `server.cfg` is installed (in the original folder or the content folder), or
/// what to do: the maps there are.
pub fn check_map(root: &std::path::Path, map: &str) -> Result<(), String> {
    let path = omsi_cfg::resolve_path(root, map);
    if omsi_cfg::vfs::exists(&path) {
        return Ok(());
    }
    Err(format!(
        "the map {} (server.cfg's map, or --server-map) is not installed ({} not found). Installed maps: {}",
        map,
        path.display(),
        Some(omsi_lan::server_cfg::installed_maps().join(", "))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "none".into())
    ))
}


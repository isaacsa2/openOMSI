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
use std::collections::{HashMap, VecDeque};
use omsi_map::{Terrain, tile_size, tile::Tile};

/// CPU data that stays alive for the lifetime of a dedicated map session.
pub struct HostWorld {
    pub global: GlobalCfg,
    pub map_dir: PathBuf,
    pub data: Arc<WorldCell>,
    pub index: Arc<MapIndex>,
    /// Bounded, CPU-only tile data; no GPU buffers or material textures.
    tiles: HostTileCache,
    tile_paths: HashMap<(i32, i32), PathBuf>,
    chrono_dirs: Vec<PathBuf>,
}

/// LRU cache for immutable OMSI tile records. A map can have tens of thousands
/// of tiles; we must not retain every .map and terrain file in RAM.
struct HostTileCache {
    limit: usize,
    order: VecDeque<(i32, i32)>,
    loaded: HashMap<(i32, i32), (Arc<Tile>, Arc<Terrain>)>,
}

impl HostTileCache {
    fn new(limit: usize) -> Self {
        Self { limit: limit.clamp(1, 512), order: VecDeque::new(), loaded: HashMap::new() }
    }

    fn get(&mut self, key: (i32, i32)) -> Option<(Arc<Tile>, Arc<Terrain>)> {
        let pair = self.loaded.get(&key)?.clone();
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        Some(pair)
    }

    fn insert(
        &mut self,
        key: (i32, i32),
        tile: Arc<Tile>,
        ground: Arc<Terrain>,
    ) -> Option<(i32, i32)> {
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        self.loaded.insert(key, (tile, ground));
        if self.loaded.len() > self.limit {
            let oldest = self.order.pop_front()?;
            self.loaded.remove(&oldest);
            return Some(oldest);
        }
        None
    }
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
        let tile_paths = files.iter().map(|(_, x, y, p)| ((*x, *y), p.clone())).collect();
        let index = Arc::new(MapIndex::build(&files, &chrono_dirs, root));
        let mut data = WorldData {
            map: Arc::new(MapFacts {
                root: root.to_path_buf(),
                map_dir: map_dir.clone(),
                raw_tiles: global.raw_tiles.clone(),
                tiles: global
                    .tiles
                    .iter()
                    .map(|t| ((t.x, t.y), t.file.clone()))
                    .collect(),
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
            index
                .objects
                .iter()
                .map(|(id, (_tile, pos, rot))| (*id, (*pos, *rot))),
            index
                .duplicates
                .iter()
                .map(|(key, (pos, rot))| (*key, (*pos, *rot))),
        );
        data.index = Some(Arc::clone(&index));

        log::info!(
            "dedicated CPU world: {} tiles indexed, {} unreadable, {} placed objects (no renderer allocated)",
            index.tiles_read,
            index.tiles_failed,
            index.objects.len()
        );
        Ok(Self {
            global,
            map_dir,
            data: Arc::new(WorldCell::new(data)),
            index,
            tiles: HostTileCache::new(64),
            tile_paths,
            chrono_dirs,
        })
    }

    /// Read one tile including Chrono patches, and publish its terrain into the
    /// simulation's ground store. CPU map/terrain parsing is lazy; repeated
    /// queries use a bounded LRU and do not retain any rendering assets.
    pub fn load_tile(&mut self, key: (i32, i32)) -> Option<Arc<Tile>> {
        if let Some((tile, _ground)) = self.tiles.get(key) {
            return Some(tile);
        }
        let path = self.tile_paths.get(&key)?;
        let tile = omsi_world::map_index::read_tile(path, &self.chrono_dirs)?;
        let source = tile.terrain_from.as_ref().unwrap_or(&tile.path);
        let ground_path = omsi_world::map_index::tile_companion(source, ".terrain");
        let ground = Arc::new(
            if tile.has_terrain {
                match Terrain::load(&ground_path) {
                    Ok(t) => t,
                    Err(e) => {
                        log::warn!("dedicated CPU terrain {}: {e}; using flat fallback", ground_path.display());
                        Terrain::flat()
                    }
                }
            } else {
                Terrain::flat()
            },
        );
        let tile = Arc::new(tile);
        let evicted = self.tiles.insert(key, Arc::clone(&tile), Arc::clone(&ground));
        self.data.update(|data| {
            let all = Arc::make_mut(&mut data.ground);
            if let Some(evicted) = evicted {
                all.terrains.remove(&evicted);
                all.surfaces.remove(&evicted);
            }
            all.terrains.insert(key, ground);
        });
        Some(tile)
    }

    /// Sample CPU terrain below a person or vehicle, even without a render
    /// device or window. Road collision overlays will be added by the physical
    /// tile loader, not silently approximated by the terrain.
    pub fn terrain_height(&mut self, x: f64, y: f64) -> Option<f32> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let key = omsi_sim::world_data::tile_key(x, y);
        self.load_tile(key)?;
        let terrain = self.tiles.loaded.get(&key).map(|(_, t)| t)?;
        Some(terrain.sample(
            (x - key.0 as f64 * tile_size()) as f32,
            (y - key.1 as f64 * tile_size()) as f32,
        ))
    }

    pub fn loaded_tiles(&self) -> usize {
        self.tiles.loaded.len()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_tile_cache_stays_bounded_and_recent_tiles_survive() {
        let mut cache = HostTileCache::new(2);
        let tile = Arc::new(Tile::default());
        let ground = Arc::new(Terrain::flat());
        assert_eq!(cache.insert((0, 0), Arc::clone(&tile), Arc::clone(&ground)), None);
        assert_eq!(cache.insert((1, 0), Arc::clone(&tile), Arc::clone(&ground)), None);
        assert!(cache.get((0, 0)).is_some());
        assert_eq!(cache.insert((2, 0), tile, ground), Some((1, 0)));
        assert!(cache.get((1, 0)).is_none());
        assert!(cache.get((0, 0)).is_some());
        assert_eq!(cache.loaded.len(), 2);
        assert_eq!(cache.order.len(), 2);
    }

    #[test]
    fn cpu_tile_cache_restricts_untrusted_capacity() {
        assert_eq!(HostTileCache::new(0).limit, 1);
        assert_eq!(HostTileCache::new(usize::MAX).limit, 512);
    }
}

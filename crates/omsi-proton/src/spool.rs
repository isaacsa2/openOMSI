//! Bounded buffered writers spool clipped faces to staging; only one tile is assembled.
use crate::{
    convert::{Faces, TILE_LIMIT},
    optimise::Builder,
    spawn::Probe,
    Spawn,
};
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};
use omsi_o3d::{Material, Mesh, Vertex};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

const RECORD: usize = 3 * 32 + 2;
const OPEN_FILES: usize = 32;

pub(crate) struct Spool {
    dir: PathBuf,
    pub tiles: BTreeMap<(i32, i32), usize>,
    pub used: BTreeSet<u16>,
    pub probe: Probe,
    writers: BTreeMap<(i32, i32), BufWriter<std::fs::File>>,
    order: VecDeque<(i32, i32)>,
}

impl Spool {
    pub fn new(dir: &Path, spawns: &[Spawn]) -> Result<Self> {
        std::fs::create_dir_all(dir.parent().unwrap())?;
        std::fs::create_dir(dir)?;
        Ok(Self {
            dir: dir.into(),
            tiles: BTreeMap::new(),
            used: BTreeSet::new(),
            probe: Probe::new(spawns),
            writers: BTreeMap::new(),
            order: VecDeque::new(),
        })
    }
    fn path(&self, key: (i32, i32)) -> PathBuf {
        self.dir.join(format!("{}_{}.faces", key.0, key.1))
    }
    pub fn finish_writes(&mut self) -> Result<()> {
        for (_, mut writer) in std::mem::take(&mut self.writers) {
            writer.flush()?;
        }
        self.order.clear();
        Ok(())
    }
    pub fn mesh(
        &self,
        key: (i32, i32),
        materials: &[Material],
        cancel: &impl Fn() -> bool,
    ) -> Result<(Mesh, usize)> {
        let count = self.tiles[&key];
        let mut reader = BufReader::with_capacity(64 * 1024, std::fs::File::open(self.path(key))?);
        ensure!(
            reader.get_ref().metadata()?.len() == (count * RECORD) as u64,
            "damaged temporary tile"
        );
        let mut builder = Builder::default();
        builder.mesh.triangles.try_reserve_exact(count)?;
        let mut bytes = [0; RECORD];
        for i in 0..count {
            if i % 4096 == 0 {
                ensure!(!cancel(), "map import cancelled");
            }
            reader.read_exact(&mut bytes)?;
            let face = std::array::from_fn(|v| {
                let floats: [f32; 8] = std::array::from_fn(|i| {
                    let p = v * 32 + i * 4;
                    f32::from_le_bytes(bytes[p..p + 4].try_into().unwrap())
                });
                Vertex {
                    position: Vec3::from_slice(&floats[..3]),
                    normal: Vec3::from_slice(&floats[3..6]),
                    uv: Vec2::from_slice(&floats[6..8]),
                }
            });
            let index = u16::from_le_bytes(bytes[RECORD - 2..].try_into()?) as usize;
            let material = materials
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("damaged temporary material index"))?;
            builder.face(face, index as u16, material);
        }
        let removed = count * 3 - builder.mesh.vertices.len();
        // Drop the lookups before writing this tile or assembling the next one.
        Ok((builder.mesh, removed))
    }
    pub fn discard_tile(&self, key: (i32, i32)) -> Result<()> {
        std::fs::remove_file(self.path(key))?;
        Ok(())
    }
}

impl Faces for Spool {
    fn face(&mut self, key: (i32, i32), vertices: [Vertex; 3], material: u16) -> Result<()> {
        ensure!(
            self.tiles.contains_key(&key) || self.tiles.len() < TILE_LIMIT,
            "map exceeds the {TILE_LIMIT} output tile import limit"
        );
        self.probe.support(key, vertices.map(|v| v.position));
        if !self.writers.contains_key(&key) {
            if self.writers.len() == OPEN_FILES {
                let old = self.order.pop_front().unwrap();
                self.writers.remove(&old).unwrap().flush()?;
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(key))?;
            self.writers
                .insert(key, BufWriter::with_capacity(64 * 1024, file));
            self.order.push_back(key);
        }
        let mut bytes = [0; RECORD];
        for (i, v) in vertices.into_iter().enumerate() {
            for (j, f) in [
                v.position.x,
                v.position.y,
                v.position.z,
                v.normal.x,
                v.normal.y,
                v.normal.z,
                v.uv.x,
                v.uv.y,
            ]
            .into_iter()
            .enumerate()
            {
                let at = i * 32 + j * 4;
                bytes[at..at + 4].copy_from_slice(&f.to_le_bytes());
            }
        }
        bytes[RECORD - 2..].copy_from_slice(&material.to_le_bytes());
        self.writers.get_mut(&key).unwrap().write_all(&bytes)?;
        *self.tiles.entry(key).or_default() += 1;
        self.used.insert(material);
        Ok(())
    }
}

impl Drop for Spool {
    fn drop(&mut self) {
        // Close handles first so cleanup also works on Windows after cancellation/failure.
        self.writers.clear();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

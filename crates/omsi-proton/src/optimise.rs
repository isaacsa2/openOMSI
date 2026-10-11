//! Compact generated tiles without changing positions, UV seams, normals or material data.
#[cfg(test)]
use crate::convert::Tiles;
#[cfg(test)]
use anyhow::{ensure, Result};
use omsi_o3d::{Material, Mesh, Vertex};
use std::collections::{BTreeMap, HashMap};

pub fn gps_name(name: &str) -> bool {
    name.split(['_', '.', '-', ' '])
        .any(|s| s.eq_ignore_ascii_case("gps"))
}

/// Drop materials belonging only to filtered helpers before resolving their textures.
#[cfg(test)]
pub fn used_materials(tiles: &mut Tiles, materials: &mut Vec<Material>) {
    let mut used = vec![false; materials.len()];
    for mesh in tiles.values() {
        for t in &mesh.triangles {
            used[t.material as usize] = true;
        }
    }
    let mut remap = vec![0; materials.len()];
    let mut next = 0u16;
    for (i, keep) in used.iter().enumerate() {
        if *keep {
            remap[i] = next;
            next += 1;
        }
    }
    let mut i = 0;
    materials.retain(|_| {
        let keep = used[i];
        i += 1;
        keep
    });
    for mesh in tiles.values_mut() {
        for t in &mut mesh.triangles {
            t.material = remap[t.material as usize];
        }
    }
}

pub(crate) fn vertex_key(v: Vertex) -> [u32; 8] {
    [
        v.position.x,
        v.position.y,
        v.position.z,
        v.normal.x,
        v.normal.y,
        v.normal.z,
        v.uv.x,
        v.uv.y,
    ]
    .map(f32::to_bits)
}

pub(crate) fn material_key(m: &Material) -> ([u32; 11], String) {
    let mut numbers = [0; 11];
    for (i, v) in m
        .diffuse
        .iter()
        .chain(&m.specular)
        .chain(&m.emissive)
        .chain([&m.specular_power])
        .enumerate()
    {
        numbers[i] = v.to_bits();
    }
    (numbers, m.texture.clone())
}

/// Build shared vertices directly, without a full three-vertices-per-face mesh or remap.
#[derive(Default)]
pub(crate) struct Builder {
    pub mesh: Mesh,
    vertices: HashMap<[u32; 8], u32>,
    materials: BTreeMap<([u32; 11], String), u16>,
    sources: HashMap<u16, u16>,
}

impl Builder {
    pub fn face(&mut self, face: [Vertex; 3], source: u16, material: &Material) {
        let next = self.mesh.materials.len() as u16;
        let slot = *self.sources.entry(source).or_insert_with(|| {
            *self
                .materials
                .entry(material_key(material))
                .or_insert_with(|| {
                    self.mesh.materials.push(material.clone());
                    next
                })
        });
        let indices = face.map(|v| {
            let key = vertex_key(v);
            if let Some(&i) = self.vertices.get(&key) {
                return i;
            }
            let i = self.mesh.vertices.len() as u32;
            if self.vertices.len() < 1_000_000 {
                self.vertices.insert(key, i);
            }
            self.mesh.vertices.push(v);
            i
        });
        self.mesh.triangles.push(omsi_o3d::Triangle {
            indices,
            material: slot,
        });
    }
}

#[cfg(test)]
fn vertices(mesh: &mut Mesh, cancel: &impl Fn() -> bool) -> Result<usize> {
    let before = mesh.vertices.len();
    let mut seen = HashMap::new();
    let mut remap = Vec::with_capacity(before);
    let mut next = 0;
    for i in 0..before {
        if i % 4096 == 0 {
            ensure!(!cancel(), "map import cancelled");
        }
        let v = mesh.vertices[i];
        let key = vertex_key(v);
        let index = match seen.get(&key) {
            Some(&index) => index,
            None => {
                let index = next as u32;
                // Bound temporary lookup memory even for a very dense tile. After this
                // point unmatched vertices remain distinct; existing matches still share.
                if seen.len() < 1_000_000 {
                    seen.insert(key, index);
                }
                mesh.vertices[next] = v;
                next += 1;
                index
            }
        };
        remap.push(index);
    }
    for t in &mut mesh.triangles {
        t.indices = t.indices.map(|i| remap[i as usize]);
    }
    mesh.vertices.truncate(next);
    mesh.vertices.shrink_to_fit();
    Ok(before - next)
}

/// Each exported O3D carries only the material slots its triangles actually reference.
#[cfg(test)]
pub fn prepare(
    tiles: &mut Tiles,
    materials: &[Material],
    cancel: &impl Fn() -> bool,
) -> Result<usize> {
    let mut removed = 0;
    for mesh in tiles.values_mut() {
        ensure!(!cancel(), "map import cancelled");
        removed += vertices(mesh, cancel)?;
        let mut seen = BTreeMap::new();
        for t in &mut mesh.triangles {
            let m = &materials[t.material as usize];
            let next = mesh.materials.len() as u16;
            let index = *seen.entry(material_key(m)).or_insert_with(|| {
                mesh.materials.push(m.clone());
                next
            });
            t.material = index;
        }
    }
    Ok(removed)
}

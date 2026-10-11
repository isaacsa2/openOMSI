//! Clip static geometry into the engine's 300 m tiles. This also keeps wheel probes and
//! visibility near the geometry instead of placing an entire city on one owner tile.
use crate::three_ds::{Model, Object};
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};
use omsi_o3d::{Material, Mesh, Triangle, Vertex};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub const EDGE: f32 = 300.0;
pub type Tiles = BTreeMap<(i32, i32), Mesh>;

fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    Vertex {
        position: a.position.lerp(b.position, t),
        normal: a.normal.lerp(b.normal, t).normalize_or_zero(),
        uv: a.uv.lerp(b.uv, t),
    }
}

fn clip(input: &[Vertex], axis: usize, edge: f32, lower: bool) -> Vec<Vertex> {
    let mut out = Vec::new();
    if input.is_empty() {
        return out;
    }
    let inside = |v: Vertex| {
        if lower {
            v.position[axis] >= edge
        } else {
            v.position[axis] <= edge
        }
    };
    let mut a = *input.last().unwrap();
    for &b in input {
        if inside(a) != inside(b) {
            let t = (edge - a.position[axis]) / (b.position[axis] - a.position[axis]);
            out.push(interpolate(a, b, t));
        }
        if inside(b) {
            out.push(b);
        }
        a = b;
    }
    out
}

fn add_triangle(
    tiles: &mut Tiles,
    vertices: [Vertex; 3],
    material: u16,
    output_vertices: &mut usize,
) -> Result<()> {
    let lo = vertices
        .iter()
        .fold(Vec3::splat(f32::INFINITY), |p, v| p.min(v.position));
    let hi = vertices
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |p, v| p.max(v.position));
    ensure!(
        lo.abs().max_element() < 1_000_000.0 && hi.abs().max_element() < 1_000_000.0,
        "map coordinate exceeds import limit"
    );
    let tx0 = (lo.x / EDGE).floor() as i32;
    let tx1 = (hi.x / EDGE).floor() as i32;
    let ty0 = (lo.y / EDGE).floor() as i32;
    let ty1 = (hi.y / EDGE).floor() as i32;
    ensure!(
        (tx1 - tx0 + 1) as i64 * (ty1 - ty0 + 1) as i64 <= 4096,
        "3DS triangle spans too many tiles"
    );
    for tx in tx0..=tx1 {
        for ty in ty0..=ty1 {
            let mut polygon = vertices.to_vec();
            for (axis, origin) in [(0, tx as f32 * EDGE), (1, ty as f32 * EDGE)] {
                polygon = clip(&polygon, axis, origin, true);
                polygon = clip(&polygon, axis, origin + EDGE, false);
            }
            for i in 1..polygon.len().saturating_sub(1) {
                let mut face = [polygon[0], polygon[i], polygon[i + 1]];
                if (face[1].position - face[0].position)
                    .cross(face[2].position - face[0].position)
                    .length_squared()
                    < 1e-12
                {
                    continue;
                }
                let mesh = tiles.entry((tx, ty)).or_default();
                *output_vertices += 3;
                ensure!(
                    *output_vertices <= 12_000_000,
                    "map exceeds the basic importer geometry limit"
                );
                ensure!(
                    mesh.vertices.len() < 6_000_000,
                    "too much geometry in one tile"
                );
                let start = mesh.vertices.len() as u32;
                for v in &mut face {
                    v.position -= Vec3::new(tx as f32 * EDGE, ty as f32 * EDGE, 0.0);
                }
                mesh.vertices.extend_from_slice(&face);
                mesh.triangles.push(Triangle {
                    indices: [start, start + 1, start + 2],
                    material,
                });
            }
        }
    }
    ensure!(tiles.len() <= 4096, "map has too many tiles");
    Ok(())
}

fn marker(name: &str, prefixes: &[String]) -> bool {
    let n = name.to_lowercase();
    prefixes.iter().any(|p| {
        let p = p.to_lowercase();
        n == p
            || n.strip_prefix(&p)
                .is_some_and(|s| s.starts_with('.') || s.starts_with('_'))
            || n.strip_prefix('_')
                .and_then(|s| s.strip_prefix(&p))
                .is_some_and(|s| s.starts_with('_'))
    })
}

fn normals(o: &Object) -> (Vec<Vec3>, Vec<Vec<usize>>) {
    let mut by_vertex = vec![Vec::new(); o.positions.len()];
    let faces = o
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| {
            for &v in f {
                by_vertex[v as usize].push(i);
            }
            let [a, b, c] = f.map(|v| o.positions[v as usize]);
            (b - a).cross(c - a)
        })
        .collect();
    (faces, by_vertex)
}

pub fn append(
    model: Model,
    tiles: &mut Tiles,
    materials: &mut Vec<Material>,
    prefixes: &[String],
    cancel: &impl Fn() -> bool,
) -> Result<usize> {
    let base = materials.len();
    ensure!(
        base + model.materials.len() + 1 < u16::MAX as usize,
        "too many map materials"
    );
    let default = base + model.materials.len();
    materials.extend(model.materials.iter().map(|(_, m)| m.clone()));
    materials.push(Material::default());
    let mut skipped = 0;
    let mut output_vertices = tiles.values().map(|m| m.vertices.len()).sum();
    for o in model.objects {
        ensure!(!cancel(), "map import cancelled");
        if marker(&o.name, prefixes) {
            skipped += 1;
            continue;
        }
        let mut face_mats = vec![default as u16; o.faces.len()];
        for (name, faces) in &o.groups {
            let index = model
                .materials
                .iter()
                .position(|(n, _)| n == name)
                .ok_or_else(|| anyhow::anyhow!("unknown 3DS material {name}"))?;
            for &face in faces {
                face_mats[face as usize] = (base + index) as u16;
            }
        }
        let (face_normals, adjacency) = normals(&o);
        for (i, f) in o.faces.iter().enumerate() {
            if i % 4096 == 0 {
                ensure!(!cancel(), "map import cancelled");
            }
            let smoothing = o.smoothing.get(i).copied().unwrap_or(0);
            let v = f.map(|j| {
                let j = j as usize;
                let normal = if smoothing == 0 {
                    face_normals[i]
                } else {
                    adjacency[j]
                        .iter()
                        .filter(|&&k| o.smoothing.get(k).copied().unwrap_or(0) & smoothing != 0)
                        .fold(Vec3::ZERO, |sum, &k| sum + face_normals[k])
                };
                Vertex {
                    position: o.positions[j],
                    normal: normal.normalize_or_zero(),
                    uv: o.uv.get(j).copied().unwrap_or(Vec2::ZERO),
                }
            });
            add_triangle(tiles, v, face_mats[i], &mut output_vertices)?;
        }
    }
    Ok(skipped)
}

/// Version 3 uses 32-bit counts and indices, without the scrambling keys of later versions.
/// Tiles use world coordinates (Z up); O3D records use Direct3D coordinates (Y up).
pub fn write_mesh(path: &Path, mesh: &Mesh, materials: &[Material]) -> Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    w.write_all(&[0x84, 0x19, 3, 1, 0x17])?;
    w.write_all(&(mesh.vertices.len() as u32).to_le_bytes())?;
    for v in &mesh.vertices {
        for f in [
            v.position.x,
            v.position.z,
            v.position.y,
            v.normal.x,
            v.normal.z,
            v.normal.y,
            v.uv.x,
            v.uv.y,
        ] {
            w.write_all(&f.to_le_bytes())?;
        }
    }
    w.write_all(&[0x49])?;
    w.write_all(&(mesh.triangles.len() as u32).to_le_bytes())?;
    for t in &mesh.triangles {
        // The renderer keeps Direct3D's clockwise front faces. Reverse the right-handed
        // 3DS face as well as swapping axes so its visible side still faces its normal.
        for i in [t.indices[0], t.indices[2], t.indices[1]] {
            w.write_all(&i.to_le_bytes())?;
        }
        w.write_all(&t.material.to_le_bytes())?;
    }
    w.write_all(&[0x26])?;
    w.write_all(&(materials.len() as u16).to_le_bytes())?;
    for m in materials {
        for f in m
            .diffuse
            .into_iter()
            .chain(m.specular)
            .chain(m.emissive)
            .chain([m.specular_power])
        {
            w.write_all(&f.to_le_bytes())?;
        }
        let (bytes, _, unmappable) = encoding_rs::WINDOWS_1252.encode(&m.texture);
        ensure!(
            !unmappable && bytes.len() <= 255,
            "texture name cannot be stored in O3D"
        );
        w.write_all(&[bytes.len() as u8])?;
        w.write_all(&bytes)?;
    }
    w.flush()?;
    Ok(())
}

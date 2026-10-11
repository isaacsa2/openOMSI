//! Clip static geometry into the engine's 300 m tiles. This also keeps wheel probes and
//! visibility near the geometry instead of placing an entire city on one owner tile.
#[cfg(test)]
use crate::three_ds::Model;
use crate::three_ds::Object;
use anyhow::{ensure, Result};
use glam::{Vec2, Vec3};
use omsi_o3d::{Material, Mesh, Triangle, Vertex};
#[cfg(test)]
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub const EDGE: f32 = 300.0;
#[cfg(test)]
pub type Tiles = BTreeMap<(i32, i32), Mesh>;
const GEOMETRY_LIMIT_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const TILE_LIMIT: usize = 16_384;
const FACE_BYTES: u64 =
    (3 * std::mem::size_of::<Vertex>() + std::mem::size_of::<Triangle>()) as u64;

fn reserve_face(output_bytes: &mut u64) -> Result<()> {
    let next = output_bytes.checked_add(FACE_BYTES);
    ensure!(
        next.is_some_and(|n| n <= GEOMETRY_LIMIT_BYTES),
        "generated map geometry exceeds the 8 GiB import limit"
    );
    *output_bytes = next.unwrap();
    Ok(())
}

fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    Vertex {
        position: a.position.lerp(b.position, t),
        normal: a.normal.lerp(b.normal, t).normalize_or_zero(),
        uv: a.uv.lerp(b.uv, t),
    }
}

fn clip(input: &[Vertex], out: &mut Vec<Vertex>, axis: usize, edge: f32, lower: bool) {
    out.clear();
    if input.is_empty() {
        return;
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
            let mut intersection = interpolate(a, b, t);
            // Large backdrops amplify float rounding; pin the cut to the shared edge.
            intersection.position[axis] = edge;
            out.push(intersection);
        }
        if inside(b) {
            out.push(b);
        }
        a = b;
    }
}

pub(crate) trait Faces {
    fn face(&mut self, key: (i32, i32), vertices: [Vertex; 3], material: u16) -> Result<()>;
    fn backdrop(&mut self, _vertices: [Vertex; 3], _material: u16) -> Result<()> {
        anyhow::bail!("3DS triangle spans too many tiles")
    }
}

#[cfg(test)]
impl Faces for Tiles {
    fn face(&mut self, key: (i32, i32), vertices: [Vertex; 3], material: u16) -> Result<()> {
        ensure!(
            self.contains_key(&key) || self.len() < TILE_LIMIT,
            "map exceeds the {TILE_LIMIT} output tile import limit"
        );
        let mesh = self.entry(key).or_default();
        let start = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_slice(&vertices);
        mesh.triangles.push(Triangle {
            indices: [start, start + 1, start + 2],
            material,
        });
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct ClipBuffers([Vec<Vertex>; 2]);

#[cfg(test)]
fn add_triangle(
    tiles: &mut Tiles,
    vertices: [Vertex; 3],
    material: u16,
    output_bytes: &mut u64,
) -> Result<()> {
    emit_triangle(
        tiles,
        vertices,
        material,
        output_bytes,
        &mut ClipBuffers::default(),
    )
}

fn emit_triangle(
    tiles: &mut impl Faces,
    vertices: [Vertex; 3],
    material: u16,
    output_bytes: &mut u64,
    scratch: &mut ClipBuffers,
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
    if (tx1 - tx0 + 1) as i64 * (ty1 - ty0 + 1) as i64 > 4096 {
        // A vast, nearly horizontal ground sheet is a backdrop, not a new city.
        // Bound its later clipping to tiles occupied by ordinary scenery.
        ensure!(hi.z - lo.z <= 1.0, "3DS triangle spans too many tiles");
        return tiles.backdrop(vertices, material);
    }
    for tx in tx0..=tx1 {
        for ty in ty0..=ty1 {
            emit_in_tile(tiles, (tx, ty), vertices, material, output_bytes, scratch)?;
        }
    }
    Ok(())
}

pub(crate) fn emit_in_tile(
    tiles: &mut impl Faces,
    (tx, ty): (i32, i32),
    vertices: [Vertex; 3],
    material: u16,
    output_bytes: &mut u64,
    scratch: &mut ClipBuffers,
) -> Result<()> {
    let [polygon, other] = &mut scratch.0;
    polygon.clear();
    polygon.extend_from_slice(&vertices);
    for (axis, origin) in [(0, tx as f32 * EDGE), (1, ty as f32 * EDGE)] {
        clip(polygon, other, axis, origin, true);
        clip(other, polygon, axis, origin + EDGE, false);
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
        reserve_face(output_bytes)?;
        for v in &mut face {
            v.position -= Vec3::new(tx as f32 * EDGE, ty as f32 * EDGE, 0.0);
        }
        tiles.face((tx, ty), face, material)?;
    }
    Ok(())
}

pub(crate) fn marker(name: &str, prefixes: &[String]) -> bool {
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

#[cfg(test)]
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
    let mut output_bytes = tiles
        .values()
        .map(|m| {
            m.vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64
                + m.triangles.len() as u64 * std::mem::size_of::<Triangle>() as u64
        })
        .sum();
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
        append_object(&o, &face_mats, &[], tiles, &mut output_bytes, cancel)?;
    }
    Ok(skipped)
}

pub(crate) fn append_object(
    o: &Object,
    face_mats: &[u16],
    mappings: &[crate::three_ds::UvTransform],
    tiles: &mut impl Faces,
    output_bytes: &mut u64,
    cancel: &impl Fn() -> bool,
) -> Result<()> {
    let (face_normals, adjacency) = normals(o);
    let mut scratch = ClipBuffers::default();
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
            let uv = o.uv.get(j).copied().unwrap_or(Vec2::ZERO);
            Vertex {
                position: o.positions[j],
                normal: normal.normalize_or_zero(),
                uv: mappings
                    .get(face_mats[i] as usize)
                    .map_or(uv, |m| m.apply(uv)),
            }
        });
        ensure!(
            v.iter().all(|v| v.uv.is_finite()),
            "non-finite transformed 3DS UV"
        );
        emit_triangle(tiles, v, face_mats[i], output_bytes, &mut scratch)?;
    }
    Ok(())
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

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn geometry_budget_rejects_the_next_face_without_changing_the_total() {
        // Cross the old 2 GiB limit without allocating multi-gigabyte geometry.
        let mut bytes = 2 * 1024 * 1024 * 1024;
        reserve_face(&mut bytes).unwrap();
        assert!(bytes > 2 * 1024 * 1024 * 1024);
        // Exercise the new boundary and reject overflow without changing the total.
        let mut bytes = GEOMETRY_LIMIT_BYTES - FACE_BYTES;
        reserve_face(&mut bytes).unwrap();
        assert_eq!(bytes, GEOMETRY_LIMIT_BYTES);
        assert!(reserve_face(&mut bytes)
            .unwrap_err()
            .to_string()
            .contains("8 GiB"));
        assert_eq!(bytes, GEOMETRY_LIMIT_BYTES);
        bytes = u64::MAX;
        assert!(reserve_face(&mut bytes).is_err());
        assert_eq!(bytes, u64::MAX);
    }

    #[test]
    fn maps_can_exceed_the_old_tile_limit_but_still_stop_at_the_new_limit() {
        let mut tiles = Tiles::new();
        let mut bytes = 0;
        for i in 0..TILE_LIMIT {
            let x = (i % 128) as f32 * EDGE + 1.0;
            let y = (i / 128) as f32 * EDGE + 1.0;
            let vertex = |dx: f32, dy: f32| Vertex {
                position: Vec3::new(x + dx, y + dy, 0.0),
                normal: Vec3::Z,
                uv: Vec2::ZERO,
            };
            add_triangle(
                &mut tiles,
                [vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
                0,
                &mut bytes,
            )
            .unwrap();
        }
        assert_eq!(tiles.len(), TILE_LIMIT);
        let vertex = |dx: f32, dy: f32| Vertex {
            position: Vec3::new(1.0 + dx, 128.0 * EDGE + 1.0 + dy, 0.0),
            normal: Vec3::Z,
            uv: Vec2::ZERO,
        };
        assert!(add_triangle(
            &mut tiles,
            [vertex(0.0, 0.0), vertex(1.0, 0.0), vertex(0.0, 1.0)],
            0,
            &mut bytes,
        )
        .unwrap_err()
        .to_string()
        .contains("16384"));
    }
}

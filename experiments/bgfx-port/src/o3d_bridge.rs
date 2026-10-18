//! Adapt the **real** openOMSI .o3d decoder to a GPU-portable mesh stream.
//!
//! We intentionally do not regenerate/reorder materials. OMSI preserves authored
//! triangle sequences for some depth-writing/transmap surfaces and glass.
//! The output is a strict, validated CPU snapshot, NOT a bgfx resource.

use std::{fs, path::Path};
use omsi_o3d::Mesh;

pub const VERTEX_STRIDE: usize = 32; // xyz, normal xyz, uv xy: all little-endian f32
const MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialRun {
    pub material: u16,
    pub first_index: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone)]
pub struct PackedO3d {
    /// Eight f32 per vertex: pos xyz, normal xyz, uv xy.
    pub vertices: Vec<u8>,
    /// 16-bit indices where possible, 32-bit where required.
    pub indices: Vec<u8>,
    pub vertex_count: u32,
    pub index_count: u32,
    pub index32: bool,
    /// Contiguous, unsorted runs in source triangle order.
    pub runs: Vec<MaterialRun>,
    /// O3D's optional object-space transform, column major. Caller must apply
    /// scene hierarchy and relative world-origin adjustment before rendering.
    pub model_transform: [f32; 16],
}

pub fn load_file(path: &Path) -> Result<PackedO3d, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mesh = omsi_o3d::parse_o3d(&bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    pack_visible_omsi_mesh(&mesh)
}

/// Apply the same O3D coordinate, material and winding rules as the shipped
/// openOMSI game. Raw O3D vertices are in x-right/y-up/z-forward coordinates;
/// the scene is x-right/y-forward/z-up. File transforms represent **animation
/// pivots**, not an additional object-to-world matrix to apply while drawing.
/// Let the established CPU geometry converter make those compatibility choices.
pub fn pack_visible_omsi_mesh(mesh: &Mesh) -> Result<PackedO3d, String> {
    for (t, triangle) in mesh.triangles.iter().enumerate() {
        for &index in &triangle.indices {
            if index as usize >= mesh.vertices.len() {
                return Err(format!("triangle {t} uses missing vertex {index}"));
            }
        }
    }
    let generated = omsi_geometry::mesh_from_o3d(mesh);
    if generated.ranges.is_empty() {
        return Err("O3D contains no visible material ranges".into());
    }
    crate::geometry_bridge::pack_generated(&generated).map(|output| output.mesh)
}

pub fn pack(mesh: &Mesh) -> Result<PackedO3d, String> {
    let vertex_count = u32::try_from(mesh.vertices.len())
        .map_err(|_| "too many vertices for bgfx".to_string())?;
    let index_count = mesh.triangles.len()
        .checked_mul(3)
        .ok_or_else(|| "triangle count overflow".to_string())?;
    let index_count = u32::try_from(index_count)
        .map_err(|_| "too many triangle indices for bgfx".to_string())?;
    if vertex_count == 0 || index_count == 0 {
        return Err("mesh has no vertices or triangles".into());
    }
    let index32 = vertex_count > (u16::MAX as u32 + 1);
    let vertex_bytes = mesh.vertices.len().checked_mul(VERTEX_STRIDE)
        .ok_or_else(|| "vertex bytes overflow".to_string())?;
    let index_bytes = (index_count as usize).checked_mul(if index32 { 4 } else { 2 })
        .ok_or_else(|| "index bytes overflow".to_string())?;
    if vertex_bytes.checked_add(index_bytes).is_none_or(|n| n > MAX_BYTES) {
        return Err("mesh exceeds 256 MiB staging limit".into());
    }
    if !mesh.transform.to_cols_array().iter().all(|f| f.is_finite()) {
        return Err("nonfinite object transform".into());
    }
    let mut vertices = Vec::with_capacity(vertex_bytes);
    for (i, v) in mesh.vertices.iter().enumerate() {
        let values = [
            v.position.x, v.position.y, v.position.z,
            v.normal.x, v.normal.y, v.normal.z,
            v.uv.x, v.uv.y,
        ];
        if !values.iter().all(|f| f.is_finite()) {
            return Err(format!("nonfinite vertex at {i}"));
        }
        for value in values {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut indices = Vec::with_capacity(index_bytes);
    let mut runs: Vec<MaterialRun> = Vec::new();
    for (t, triangle) in mesh.triangles.iter().enumerate() {
        // Source material references above the table are preserved: the scene
        // material resolver may supply a fallback (as OMSI does).
        for &index in &triangle.indices {
            if index >= vertex_count {
                return Err(format!("triangle {t} uses vertex {index} >= {vertex_count}"));
            }
            if index32 {
                indices.extend_from_slice(&index.to_le_bytes());
            } else {
                let short = u16::try_from(index).map_err(|_| "index width mismatch")?;
                indices.extend_from_slice(&short.to_le_bytes());
            }
        }
        match runs.last_mut() {
            Some(run) if run.material == triangle.material => run.index_count += 3,
            _ => runs.push(MaterialRun {
                material: triangle.material,
                first_index: u32::try_from(t * 3).map_err(|_| "index offset overflow")?,
                index_count: 3,
            }),
        }
    }
    debug_assert_eq!(indices.len(), index_bytes);
    debug_assert_eq!(vertices.len(), vertex_bytes);
    Ok(PackedO3d {
        vertices,
        indices,
        vertex_count,
        index_count,
        index32,
        runs,
        model_transform: mesh.transform.to_cols_array(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_o3d::{Triangle, Vertex};

    fn example() -> Mesh {
        let mut mesh = Mesh::default();
        mesh.vertices = vec![
            Vertex { position: glam::Vec3::new(0., 0., 0.), normal: glam::Vec3::Z, uv: glam::Vec2::ZERO },
            Vertex { position: glam::Vec3::new(1., 0., 0.), normal: glam::Vec3::Z, uv: glam::Vec2::X },
            Vertex { position: glam::Vec3::new(0., 1., 0.), normal: glam::Vec3::Z, uv: glam::Vec2::Y },
        ];
        mesh.triangles = vec![
            Triangle { indices: [0,1,2], material: 3 },
            Triangle { indices: [2,1,0], material: 7 },
            Triangle { indices: [0,1,2], material: 3 },
        ];
        mesh
    }

    #[test]
    fn preserves_triangle_material_order() {
        let out = pack(&example()).unwrap();
        assert_eq!(out.vertex_count, 3);
        assert_eq!(out.index_count, 9);
        assert_eq!(out.vertices.len(), 96);
        assert_eq!(out.indices.len(), 18);
        assert_eq!(out.runs.iter().map(|r| (r.material, r.first_index, r.index_count)).collect::<Vec<_>>(),
            [(3,0,3), (7,3,3), (3,6,3)]);
    }

    #[test]
    fn rejects_bad_indices_instead_of_corrupting_gpu_buffers() {
        let mut m = example();
        m.triangles[1].indices[2] = 14;
        assert!(pack(&m).unwrap_err().contains("triangle 1"));
    }

    #[test]
    fn visible_omsi_mesh_uses_production_axes_and_ignores_animation_pivot() {
        let mut m = example();
        m.materials = vec![omsi_o3d::Material::default(); 8];
        m.vertices[0].position = glam::Vec3::new(1., 2., 3.);
        m.transform = glam::Mat4::from_translation(glam::Vec3::new(400., 500., 600.));
        m.has_transform = true;
        let out = pack_visible_omsi_mesh(&m).unwrap();
        // OMSI/D3D Y is height, Z is forward; in the rendered game Z is up.
        let read = |at: usize| f32::from_le_bytes(out.vertices[at..at+4].try_into().unwrap());
        assert_eq!([read(0), read(4), read(8)], [1., 3., 2.]);
        assert_eq!(out.model_transform, glam::Mat4::IDENTITY.to_cols_array());
        // Same material buckets as the shipping engine: groups 3 and 7.
        assert_eq!(out.runs.iter().map(|r| r.material).collect::<Vec<_>>(), [3, 7]);
    }

    #[test]
    fn visible_mesh_rejects_corrupt_triangle_before_winding_probe() {
        let mut m = example();
        m.materials = vec![omsi_o3d::Material::default()];
        m.triangles[0].indices[1] = 99999;
        assert!(pack_visible_omsi_mesh(&m).is_err());
    }

    #[test]
    fn rejects_nonfinite_vertices() {
        let mut m = example();
        m.vertices[0].position.x = f32::NAN;
        assert!(pack(&m).unwrap_err().contains("nonfinite vertex"));
    }
}

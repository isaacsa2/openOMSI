//! Translate the actual openOMSI CPU terrain/spline/generated geometry into
//! BGFX-ready vertex/index ranges. Does not load a map yet.
//!
//! The source `MeshData` comes from `omsi-geometry`, which is independent
//! of wgpu; this is the seam where a second renderer can reuse maps.

use crate::o3d_bridge::{self, PackedO3d};
use omsi_geometry::MeshData;
use omsi_o3d::{Mesh, Triangle, Vertex};

#[derive(Debug, Clone)]
pub struct PackedGeneratedMesh {
    pub mesh: PackedO3d,
    pub one_sided: bool,
}

/// Preserve source range ordering and material slots. Generated geometry has
/// faces that may be two-sided (terrain, grass, splines): the GPU renderer must
/// respect one_sided when choosing culling rather than forcing it on.
pub fn pack_generated(input: &MeshData) -> Result<PackedGeneratedMesh, String> {
    let n = input.positions.len();
    if n == 0 || input.indices.is_empty() {
        return Err("generated mesh contains no triangles".into());
    }
    if !input.normals.is_empty() && input.normals.len() != n {
        return Err("normal array length disagrees with vertex positions".into());
    }
    if !input.uvs.is_empty() && input.uvs.len() != n {
        return Err("UV array length disagrees with vertex positions".into());
    }
    // Bound the data *before* allocating a second copy for the FFI staging
    // format. Generated maps can have millions of triangles.
    let bytes = n.checked_mul(32)
        .and_then(|v| input.indices.len().checked_mul(4).and_then(|i| v.checked_add(i)))
        .ok_or_else(|| "generated mesh staging byte count overflow".to_string())?;
    if bytes > 256 * 1024 * 1024 {
        return Err("generated mesh exceeds per-mesh 256 MiB staging budget".into());
    }
    let mut mesh = Mesh::default();
    mesh.transform = glam::Mat4::IDENTITY;
    mesh.vertices.reserve(n);
    for (index, &position) in input.positions.iter().enumerate() {
        let normal = input.normals.get(index).copied().unwrap_or(glam::Vec3::Z);
        let uv = input.uvs.get(index).copied().unwrap_or(glam::Vec2::ZERO);
        mesh.vertices.push(Vertex { position, normal, uv });
    }
    let mut last_end = 0usize;
    for &(first, count, material_slot) in &input.ranges {
        let first = first as usize;
        let count = count as usize;
        let end = first.checked_add(count)
            .ok_or_else(|| "generated material range overflow".to_string())?;
        if end > input.indices.len() || count % 3 != 0 || first % 3 != 0 {
            return Err("generated material range invalid or not triangle-aligned".into());
        }
        // Some author-defined geometry may have multiple source ranges.
        // No reordering, overlaps or gaps: draw exactly what the source lists.
        if first != last_end {
            return Err("generated source has noncontiguous/overlapping material ranges".into());
        }
        let slot = u16::try_from(material_slot)
            .map_err(|_| "generated material slot overflows u16".to_string())?;
        for chunk in input.indices[first..end].chunks_exact(3) {
            mesh.triangles.push(Triangle { indices: [chunk[0], chunk[1], chunk[2]], material: slot });
        }
        last_end = end;
    }
    if last_end != input.indices.len() {
        return Err("generated source has indices outside material runs".into());
    }
    Ok(PackedGeneratedMesh { mesh: o3d_bridge::pack(&mesh)?, one_sided: input.one_sided })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_generated_terrain_geometry_without_winding_changes() {
        let source = MeshData {
            positions: vec![glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y],
            normals: vec![glam::Vec3::Z; 3],
            uvs: vec![glam::Vec2::ZERO; 3],
            ranges: vec![(0, 3, 2)],
            indices: vec![2, 0, 1],
            one_sided: false,
        };
        let result = pack_generated(&source).unwrap();
        assert!(!result.one_sided);
        assert_eq!(result.mesh.runs[0].material, 2);
        assert_eq!(&result.mesh.indices, &[2, 0, 0, 0, 1, 0]);
    }

    #[test]
    fn bad_material_ranges_are_rejected() {
        let source = MeshData {
            positions: vec![glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y],
            normals: vec![],
            uvs: vec![],
            ranges: vec![(1, 3, 2)],
            indices: vec![0, 1, 2],
            one_sided: false,
        };
        assert!(pack_generated(&source).is_err());
    }
}

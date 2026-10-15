//! A reusable broad phase for rays through an immutable mesh. Leaves still use the same
//! triangle test as `ray_mesh`; animated parts move the ray, not the tree.

use super::{ray_triangle, Mat4, MeshData, Vec3};

#[derive(Debug, Clone)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    first: usize,
    count: usize,
    children: Option<(usize, usize)>,
}

/// Triangle bounds built once for repeated picking of a vehicle's body or cockpit.
/// Rebuild this when the mesh's positions or indices change.
#[derive(Debug, Clone, Default)]
pub struct MeshRaycast {
    nodes: Vec<Node>,
    triangles: Vec<usize>,
}

impl MeshRaycast {
    pub fn new(mesh: &MeshData) -> Self {
        let mut tree = Self {
            nodes: Vec::new(),
            triangles: (0..mesh.indices.len() / 3).map(|i| i * 3).collect(),
        };
        if !tree.triangles.is_empty() {
            tree.build(mesh, 0, tree.triangles.len());
        }
        tree
    }

    fn build(&mut self, mesh: &MeshData, first: usize, count: usize) -> usize {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for &k in &self.triangles[first..first + count] {
            for &i in &mesh.indices[k..k + 3] {
                lo = lo.min(mesh.positions[i as usize]);
                hi = hi.max(mesh.positions[i as usize]);
            }
        }
        // Conservative at shared edges and at the magnitudes used by imported models.
        let pad = lo.abs().max(hi.abs()) * (8.0 * f32::EPSILON) + Vec3::splat(1e-4);
        let node = self.nodes.len();
        self.nodes.push(Node {
            lo: lo - pad,
            hi: hi + pad,
            first,
            count,
            children: None,
        });
        if count > 8 {
            let extent = hi - lo;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let middle = count / 2;
            let centre = |k: usize| {
                mesh.indices[k..k + 3]
                    .iter()
                    .map(|&i| mesh.positions[i as usize][axis])
                    .sum::<f32>()
                    / 3.0
            };
            self.triangles[first..first + count]
                .select_nth_unstable_by(middle, |a, b| centre(*a).total_cmp(&centre(*b)));
            let left = self.build(mesh, first, middle);
            let right = self.build(mesh, first + middle, count - middle);
            self.nodes[node].children = Some((left, right));
        }
        node
    }

    /// The same distance and triangle epsilon as `ray_mesh`, including scaled parts.
    pub fn hit(&self, origin: Vec3, dir: Vec3, mesh: &MeshData, transform: &Mat4) -> Option<f32> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = transform.inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(dir);
        let scale = d.length();
        if scale < 1e-9 {
            return None;
        }
        let mut best = f32::INFINITY;
        self.trace(0, o, d / scale, mesh, &mut best);
        best.is_finite().then_some(best / scale)
    }

    /// The same candidates, in index order, as `cone_triangles`, without scanning the
    /// entire cockpit panel before trying the cursor's rings of rays.
    pub fn cone_triangles(
        &self,
        origin: Vec3,
        axis: Vec3,
        tan_half: f32,
        mesh: &MeshData,
        transform: &Mat4,
    ) -> Vec<u32> {
        let mut out = Vec::new();
        if !self.nodes.is_empty() {
            let inv = transform.inverse();
            let o = inv.transform_point3(origin);
            let a = inv.transform_vector3(axis).normalize_or_zero();
            self.collect_cone(0, o, a, tan_half, mesh, &mut out);
            out.sort_unstable();
        }
        out
    }

    fn collect_cone(
        &self,
        index: usize,
        o: Vec3,
        a: Vec3,
        tan_half: f32,
        mesh: &MeshData,
        out: &mut Vec<u32>,
    ) {
        let node = &self.nodes[index];
        let c = (node.lo + node.hi) * 0.5;
        // Each triangle's centroid is in this box (radius R) and its sphere's
        // radius is at most 2R. Use 3R to contain those spheres too: testing just
        // the box's own sphere could drop candidates accepted by the old filter.
        let r = (node.hi - node.lo).length() * 1.5;
        if !in_cone(o, a, tan_half, c, r) {
            return;
        }
        if let Some((left, right)) = node.children {
            self.collect_cone(left, o, a, tan_half, mesh, out);
            self.collect_cone(right, o, a, tan_half, mesh, out);
        } else {
            for &k in &self.triangles[node.first..node.first + node.count] {
                let p = |i| mesh.positions[mesh.indices[k + i] as usize];
                let (p0, p1, p2) = (p(0), p(1), p(2));
                let c = (p0 + p1 + p2) / 3.0;
                let r = (p0 - c)
                    .length()
                    .max((p1 - c).length())
                    .max((p2 - c).length());
                if in_cone(o, a, tan_half, c, r) {
                    out.push(k as u32);
                }
            }
        }
    }

    fn trace(&self, index: usize, o: Vec3, d: Vec3, mesh: &MeshData, best: &mut f32) {
        let node = &self.nodes[index];
        if !intersects(node, o, d, *best) {
            return;
        }
        if let Some((left, right)) = node.children {
            self.trace(left, o, d, mesh, best);
            self.trace(right, o, d, mesh, best);
        } else {
            for &k in &self.triangles[node.first..node.first + node.count] {
                let p = |i| mesh.positions[mesh.indices[k + i] as usize];
                if let Some(t) = ray_triangle(o, d, p(0), p(1), p(2)) {
                    *best = best.min(t);
                }
            }
        }
    }
}

fn in_cone(o: Vec3, a: Vec3, tan_half: f32, c: Vec3, r: f32) -> bool {
    let w = c - o;
    let along = w.dot(a);
    if along < -r {
        return false;
    }
    let across = (w - a * along).length();
    !(across > (along + r).max(0.0) * tan_half + r + 1e-3)
}

fn intersects(node: &Node, o: Vec3, d: Vec3, best: f32) -> bool {
    let (mut near, mut far) = (0.0_f32, best);
    for axis in 0..3 {
        if d[axis] == 0.0 {
            if o[axis] < node.lo[axis] || o[axis] > node.hi[axis] {
                return false;
            }
        } else {
            let a = (node.lo[axis] - o[axis]) / d[axis];
            let b = (node.hi[axis] - o[axis]) / d[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
#[path = "mesh_ray_tests.rs"]
mod tests;

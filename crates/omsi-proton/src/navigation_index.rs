//! Bounded spatial lookup for optional 2D overlays; never a traffic network.
use std::collections::BTreeMap;
type Triangle = [[f32; 2]; 3];
const CELL: f32 = 256.0;
const MAX_REFERENCES: usize = 250_000;

#[derive(Default)]
pub struct TriangleIndex {
    cells: BTreeMap<(i32, i32), Vec<usize>>,
    wide: Vec<usize>,
    extent: Option<([f32; 2], [f32; 2])>,
}

fn bounds(triangle: &Triangle) -> ([f32; 2], [f32; 2]) {
    let mut lo = triangle[0];
    let mut hi = lo;
    for p in triangle.iter().skip(1) {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}
fn cell(p: [f32; 2]) -> (i32, i32) {
    ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32)
}

impl TriangleIndex {
    pub fn new(triangles: &[Triangle]) -> Self {
        let mut index = Self::default();
        let mut references = 0;
        for (i, t) in triangles.iter().enumerate() {
            if !t
                .iter()
                .flatten()
                .all(|n| n.is_finite() && n.abs() < 1_000_000.)
            {
                continue;
            }
            let (lo, hi) = bounds(t);
            index.extent = Some(match index.extent {
                None => (lo, hi),
                Some((a, b)) => (
                    [a[0].min(lo[0]), a[1].min(lo[1])],
                    [b[0].max(hi[0]), b[1].max(hi[1])],
                ),
            });
            let ((x0, y0), (x1, y1)) = (cell(lo), cell(hi));
            let count = (x1 as i64 - x0 as i64 + 1).saturating_mul(y1 as i64 - y0 as i64 + 1);
            if count > 64 || references + count as usize > MAX_REFERENCES {
                index.wide.push(i);
                continue;
            }
            for x in x0..=x1 {
                for y in y0..=y1 {
                    index.cells.entry((x, y)).or_default().push(i);
                }
            }
            references += count as usize;
        }
        index
    }
    pub fn extent(&self) -> Option<([f32; 2], [f32; 2])> {
        self.extent
    }

    /// Return each visible triangle once, including triangles spanning the query.
    pub fn nearby(&self, triangles: &[Triangle], center: [f32; 2], radius: f32) -> Vec<usize> {
        if !radius.is_finite() || radius < 0. || !center.iter().all(|n| n.is_finite()) {
            return Vec::new();
        }
        let lo = [center[0] - radius, center[1] - radius];
        let hi = [center[0] + radius, center[1] + radius];
        let ((x0, y0), (x1, y1)) = (cell(lo), cell(hi));
        let count = (x1 as i64 - x0 as i64 + 1).saturating_mul(y1 as i64 - y0 as i64 + 1);
        let mut found = if count > 4096 {
            (0..triangles.len()).collect()
        } else {
            let mut found = self.wide.clone();
            for x in x0..=x1 {
                for y in y0..=y1 {
                    if let Some(ids) = self.cells.get(&(x, y)) {
                        found.extend_from_slice(ids);
                    }
                }
            }
            found.sort_unstable();
            found.dedup();
            found
        };
        found.retain(|&i| {
            let Some(t) = triangles.get(i) else {
                return false;
            };
            let (a, b) = bounds(t);
            a[0] <= hi[0] && a[1] <= hi[1] && b[0] >= lo[0] && b[1] >= lo[1]
        });
        found
    }
}

/// Match full route names with separators tolerated, never a numeric prefix.
fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(str::to_lowercase)
        .collect()
}
pub(crate) fn route_in(name: &str, routes: &[crate::navigation::Route]) -> Vec<usize> {
    let tokens = words(name);
    let name = tokens
        .iter()
        .position(|p| p == "gps")
        .map(|i| &tokens[i + 1..])
        .unwrap_or(&tokens);
    let mut matches = Vec::new();
    let mut longest = 0;
    for (i, r) in routes.iter().enumerate() {
        let tag = words(&r.name);
        if tag.is_empty() || tag.len() > name.len() || !name.windows(tag.len()).any(|w| w == tag) {
            continue;
        }
        if tag.len() > longest {
            matches.clear();
            longest = tag.len();
        }
        if tag.len() == longest {
            matches.push(i);
        }
    }
    matches
}
pub(crate) fn explicit_gps_route(name: &str) -> bool {
    let name = words(name);
    name.iter()
        .position(|p| p == "gps")
        .is_some_and(|i| i + 1 < name.len())
}
pub(crate) fn equivalent(a: &str, b: &str) -> bool {
    words(a) == words(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_queries_keep_crossing_triangles_once_and_bound_large_faces() {
        let triangles = vec![
            [[-300., 0.], [300., 0.], [0., 300.]],
            [[10000., 0.], [10020., 0.], [10000., 20.]],
            [[-900000., -900000.], [900000., 0.], [0., 900000.]],
        ];
        let index = TriangleIndex::new(&triangles);
        assert_eq!(index.nearby(&triangles, [0., 0.], 10.), vec![0, 2]);
        assert_eq!(index.wide, vec![2]);
        assert!(index.cells.len() < 100);
        assert_eq!(
            index.extent(),
            Some(([-900000., -900000.], [900000., 900000.]))
        );
    }
    #[test]
    fn longest_name_wins_and_normalized_collisions_remain_ambiguous() {
        let routes = ["101", "101 Downtown", "Centro Bairro", "Centro_Bairro"].map(|name| {
            crate::navigation::Route {
                name: name.into(),
                triangles: vec![],
            }
        });
        assert_eq!(route_in("gps_101_Downtown.3ds", &routes), vec![1]);
        assert!(route_in("route_gps_1010_part1", &routes).is_empty());
        assert_eq!(route_in("GPS-centro-bairro-part1", &routes), vec![2, 3]);
    }
}

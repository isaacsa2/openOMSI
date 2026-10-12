//! Small, versioned navigation data, separate from collision and render geometry.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub use crate::navigation_index::TriangleIndex;

pub const FILE: &str = "proton-import/navigation.jsonoi";
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_TRIANGLES: usize = 250_000;
pub(crate) const MAX_POINTS: usize = 250_000;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Navigation {
    pub version: u32,
    pub routes: Vec<Route>,
    pub roads: Vec<Road>,
    /// Optional approximate road footprints for map drawing, never traffic or routing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub road_triangles: Vec<[[f32; 2]; 3]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Route {
    pub name: String,
    /// World east/north coordinates; elevation is irrelevant on the map.
    pub triangles: Vec<[[f32; 2]; 3]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Road {
    pub name: String,
    /// World east/north/up, like the generated scenery.
    pub points: Vec<[f32; 3]>,
    /// Numeric markers with unknown manual links are overview geometry only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overview_only: bool,
}

impl Navigation {
    pub fn route_index(&self, entry: &str) -> Option<usize> {
        if let Some(i) = self.routes.iter().position(|r| r.name == entry) {
            return Some(i);
        }
        let mut found = self
            .routes
            .iter()
            .enumerate()
            .filter(|(_, r)| crate::navigation_index::equivalent(&r.name, entry));
        let i = found.next()?.0;
        found.next().is_none().then_some(i)
    }
    /// Validate before the engine uses this optional data. Missing or invalid data
    /// must not prevent scenery from loading; callers log and ignore the error.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "navigation data exceeds 64 MiB");
        let data: Self = serde_json::from_slice(bytes)?;
        ensure!(data.version == 1, "unsupported navigation version");
        ensure!(
            data.routes.len() <= 4096 && data.roads.len() <= 4096,
            "too many navigation groups"
        );
        let triangles: usize = data.routes.iter().map(|r| r.triangles.len()).sum();
        let points: usize = data.roads.iter().map(|r| r.points.len()).sum();
        ensure!(
            triangles + data.road_triangles.len() <= MAX_TRIANGLES
                && data.road_triangles.len() <= 32768
                && points <= MAX_POINTS,
            "navigation geometry exceeds budget"
        );
        let valid = |x: &f32| x.is_finite() && x.abs() < 1_000_000.0;
        ensure!(
            data.routes
                .iter()
                .flat_map(|r| &r.triangles)
                .flatten()
                .flatten()
                .all(valid),
            "invalid GPS coordinate"
        );
        ensure!(
            data.roads
                .iter()
                .all(|r| r.points.len() >= 2 && r.points.iter().flatten().all(valid)),
            "invalid road path"
        );
        ensure!(
            data.road_triangles.iter().flatten().flatten().all(valid),
            "invalid road surface coordinate"
        );
        Ok(data)
    }
}

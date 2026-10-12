//! Small, versioned navigation data, separate from collision and render geometry.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub const FILE: &str = "proton-import/navigation.jsonoi";
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_TRIANGLES: usize = 250_000;
pub(crate) const MAX_POINTS: usize = 250_000;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Navigation {
    pub version: u32,
    pub routes: Vec<Route>,
    pub roads: Vec<Road>,
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
}

impl Navigation {
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
            triangles <= MAX_TRIANGLES && points <= MAX_POINTS,
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
        Ok(data)
    }
}

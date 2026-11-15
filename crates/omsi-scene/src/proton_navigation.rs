//! Optional road geometry from generated maps.
use glam::DVec3;
use omsi_proton::navigation::Navigation;
use omsi_sim::traffic::{Lane, LaneBuilder, LaneKind};
use std::{path::PathBuf, sync::Arc};

pub(crate) fn load(
    tiles: &[(i32, i32, PathBuf)],
    lanes: &mut Vec<Lane>,
) -> Option<Arc<Navigation>> {
    let dir = tiles.first()?.2.parent()?;
    let file = dir.join(omsi_proton::navigation::FILE);
    let bytes = omsi_cfg::vfs::file_size(&file)?;
    let read = || -> anyhow::Result<Navigation> {
        anyhow::ensure!(
            bytes <= omsi_proton::navigation::MAX_BYTES as u64 + omsi_cfg::oi::HEADER_SIZE as u64,
            "navigation data exceeds budget"
        );
        Navigation::from_bytes(&omsi_cfg::vfs::read(&file)?)
    };
    let data = match read() {
        Ok(data) => data,
        Err(error) => {
            log::warn!("ignoring {}: {error}", file.display());
            return None;
        }
    };
    for road in data.roads.iter().filter(|road| !road.overview_only) {
        let points = road
            .points
            .iter()
            .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
            .collect();
        let mut lane = LaneBuilder::polyline(points, LaneKind::Street, 3.5);
        lane.name = road.name.clone();
        lane.density = 0.0;
        lanes.push(lane);
    }
    Some(Arc::new(data))
}

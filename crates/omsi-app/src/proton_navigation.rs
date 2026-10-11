//! Optional, bounded map overlays. These paths never enter the driving/AI network.
use glam::{DVec2, DVec3};
use omsi_proton::navigation::Navigation;
use omsi_sim::traffic::{Lane, LaneBuilder, LaneKind};
use omsi_ui::{paint::Vertex, Color, Painter};
use std::{path::PathBuf, sync::Arc};

pub(crate) fn load(
    tiles: &[(i32, i32, PathBuf)],
    lanes: &mut Vec<Lane>,
) -> Option<Arc<Navigation>> {
    let dir = tiles.first()?.2.parent()?;
    let file = dir.join(omsi_proton::navigation::FILE);
    let meta = std::fs::metadata(&file).ok()?;
    let read = || -> anyhow::Result<Navigation> {
        anyhow::ensure!(
            meta.len() <= omsi_proton::navigation::MAX_BYTES as u64,
            "navigation data exceeds budget"
        );
        Navigation::from_bytes(&std::fs::read(&file)?)
    };
    let data = match read() {
        Ok(data) => data,
        Err(error) => {
            log::warn!("ignoring {}: {error}", file.display());
            return None;
        }
    };
    for road in &data.roads {
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

#[derive(Default)]
pub(crate) struct Gps {
    pub data: Option<Arc<Navigation>>,
    entry: String,
    /// Changes in selection invalidate the navigator's cached map buffers.
    pub version: u64,
}

impl Gps {
    pub fn cache_key(&self) -> u64 {
        self.version | (1 << 63)
    }

    pub fn extent(&self) -> Option<(DVec2, DVec2)> {
        let route = self
            .data
            .as_ref()?
            .routes
            .iter()
            .find(|r| r.name == self.entry)?;
        let mut points = route
            .triangles
            .iter()
            .flatten()
            .map(|p| DVec2::new(p[0] as f64, p[1] as f64));
        let first = points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    pub fn select(&mut self, entry: &str) {
        if self.entry != entry {
            self.entry = entry.to_owned();
            self.version += 1;
        }
    }

    pub fn paint(&self, painter: &mut Painter, anchor: DVec2, radius: Option<f64>, color: Color) {
        let Some(data) = &self.data else { return };
        let Some(route) = data.routes.iter().find(|r| r.name == self.entry) else {
            return;
        };
        for triangle in &route.triangles {
            let points = triangle.map(|p| DVec2::new(p[0] as f64, p[1] as f64) - anchor);
            if let Some(radius) = radius {
                let lo = points.iter().copied().reduce(DVec2::min).unwrap();
                let hi = points.iter().copied().reduce(DVec2::max).unwrap();
                if lo.x > radius || lo.y > radius || hi.x < -radius || hi.y < -radius {
                    continue;
                }
            }
            painter.verts.extend(points.map(|p| Vertex {
                pos: [p.x as f32, p.y as f32, 0.0],
                color: color.0,
                mode: [1.0, 0.0],
                ..Default::default()
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_proton::navigation::Route;

    #[test]
    fn roads_load_for_navigation_without_starting_ai_and_broken_optional_data_is_ignored() {
        let dir = std::env::temp_dir().join(format!(
            "openomsi-navigation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("proton-import")).unwrap();
        std::fs::write(
            dir.join("global.cfg"),
            "[name]\nNavigation fixture\n[map]\n0\n0\ntile_0_0.map\n",
        )
        .unwrap();
        std::fs::write(dir.join("tile_0_0.map"), "[version]\n14\n").unwrap();
        let file = dir.join(omsi_proton::navigation::FILE);
        std::fs::write(&file,br#"{"version":1,"routes":[],"roads":[{"name":"main.txt","points":[[10,20,7],[30,40,8]]}]}"#).unwrap();
        let world = crate::scene::World::open(&dir, &dir.join("global.cfg"), 20261001).unwrap();
        let map = world.navigation_map();
        assert_eq!(map.lanes.len(), 1);
        assert_eq!(
            map.lanes[0].points,
            vec![DVec3::new(10.0, 20.0, 7.0), DVec3::new(30.0, 40.0, 8.0)]
        );
        assert_eq!(map.lanes[0].density, 0.0);
        assert!(world.lanes.lock().is_empty());
        let (roads, _) = crate::navigator::city_roads(map.lanes, &[]);
        assert_eq!(roads.len(), 1);
        assert_eq!(roads[0].width, 3.5);
        std::fs::write(&file, "damaged data").unwrap();
        let map = world.navigation_map();
        assert!(map.lanes.is_empty() && map.imported.is_none());
        std::fs::remove_file(file).unwrap();
        assert!(world.navigation_map().imported.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn gps_selects_entry_and_clips_nearby_without_losing_a_crossing_triangle() {
        let mut gps = Gps::default();
        gps.data = Some(Arc::new(Navigation {
            version: 1,
            roads: vec![],
            routes: vec![
                Route {
                    name: "101 Downtown".into(),
                    triangles: vec![
                        [[-100.0, 0.0], [100.0, 0.0], [0.0, 100.0]],
                        [[1000.0, 0.0], [1100.0, 0.0], [1000.0, 100.0]],
                    ],
                },
                Route {
                    name: "202 Airport".into(),
                    triangles: vec![[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
                },
            ],
        }));
        let mut painter = Painter::new();
        gps.select("101 Downtown");
        gps.paint(&mut painter, DVec2::ZERO, Some(20.0), Color::WHITE);
        assert_eq!(painter.len(), 3);
        assert_eq!(
            gps.extent(),
            Some((DVec2::new(-100.0, 0.0), DVec2::new(1100.0, 100.0)))
        );
        assert_eq!(painter.verts[0].pos, [-100.0, 0.0, 0.0]);
        assert!(painter.verts.iter().all(|v| v.mode == [1.0, 0.0]));
        let version = gps.version;
        gps.select("101 Downtown");
        assert_eq!(gps.version, version);
        painter.clear();
        gps.select("202 Airport");
        assert!(gps.version > version);
        gps.paint(&mut painter, DVec2::new(1.0, 2.0), None, Color::WHITE);
        assert_eq!(painter.verts[0].pos, [-1.0, -2.0, 0.0]);
        painter.clear();
        gps.select("Missing");
        gps.paint(&mut painter, DVec2::ZERO, None, Color::WHITE);
        assert!(painter.is_empty());
    }
}

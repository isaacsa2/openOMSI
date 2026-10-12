//! Optional, bounded map overlays. These paths never enter the driving/AI network.
use glam::DVec2;
use omsi_proton::navigation::Navigation;
use omsi_ui::{paint::Vertex, Color, Painter};
use std::sync::Arc;

#[derive(Default)]
pub struct Gps {
    data: Option<Arc<Navigation>>,
    entry: String,
    pub version: u64,
    selected: Option<usize>,
    route_index: omsi_proton::navigation::TriangleIndex,
    road_index: omsi_proton::navigation::TriangleIndex,
}

impl Gps {
    pub fn has_data(&self) -> bool {
        self.data.is_some()
    }
    pub fn cache_key(&self) -> u64 {
        self.version | (1 << 63)
    }
    pub fn set_data(&mut self, data: Option<Arc<Navigation>>) {
        self.data = data;
        self.road_index = omsi_proton::navigation::TriangleIndex::new(
            self.data
                .as_ref()
                .map(|d| d.road_triangles.as_slice())
                .unwrap_or(&[]),
        );
        self.reselect();
        self.version = self.version.wrapping_add(1);
    }
    fn reselect(&mut self) {
        self.selected = self.data.as_ref().and_then(|d| d.route_index(&self.entry));
        self.route_index = omsi_proton::navigation::TriangleIndex::new(
            self.data
                .as_ref()
                .and_then(|d| self.selected.map(|i| d.routes[i].triangles.as_slice()))
                .unwrap_or(&[]),
        );
    }
    pub fn extent(&self) -> Option<(DVec2, DVec2)> {
        self.route_index.extent().map(|(a, b)| {
            (
                DVec2::from_array(a.map(f64::from)),
                DVec2::from_array(b.map(f64::from)),
            )
        })
    }
    pub fn map_extent(&self) -> Option<(DVec2, DVec2)> {
        let roads = self.road_index.extent().map(|(a, b)| {
            (
                DVec2::from_array(a.map(f64::from)),
                DVec2::from_array(b.map(f64::from)),
            )
        });
        let mut roads = roads;
        if let Some(data) = &self.data {
            for p in data
                .roads
                .iter()
                .filter(|r| r.overview_only)
                .flat_map(|r| &r.points)
            {
                let p = DVec2::new(p[0] as f64, p[1] as f64);
                roads = Some(roads.map(|(a, b)| (a.min(p), b.max(p))).unwrap_or((p, p)));
            }
        }
        match (roads, self.extent()) {
            (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
            (a, b) => a.or(b),
        }
    }
    pub fn select(&mut self, entry: &str) {
        if self.entry != entry {
            self.entry = entry.to_owned();
            self.reselect();
            self.version = self.version.wrapping_add(1);
        }
    }
    pub fn paint(&self, painter: &mut Painter, anchor: DVec2, radius: Option<f64>, color: Color) {
        let Some(data) = &self.data else {
            return;
        };
        let Some(i) = self.selected else {
            return;
        };
        paint_triangles(
            painter,
            &data.routes[i].triangles,
            &self.route_index,
            anchor,
            radius,
            color,
        );
    }
    pub fn paint_roads(
        &self,
        painter: &mut Painter,
        anchor: DVec2,
        radius: Option<f64>,
        color: Color,
    ) {
        let Some(data) = &self.data else {
            return;
        };
        for road in data.roads.iter().filter(|r| r.overview_only) {
            for pair in road.points.windows(2) {
                let a = DVec2::new(pair[0][0] as f64, pair[0][1] as f64) - anchor;
                let b = DVec2::new(pair[1][0] as f64, pair[1][1] as f64) - anchor;
                if radius.is_some_and(|r| {
                    a.min(b).x > r || a.min(b).y > r || a.max(b).x < -r || a.max(b).y < -r
                }) {
                    continue;
                }
                let points = [
                    glam::Vec3::new(a.x as f32, a.y as f32, 0.),
                    glam::Vec3::new(b.x as f32, b.y as f32, 0.),
                ];
                painter.ribbon(&points, 3.5, 1.4, color, true);
            }
        }
        paint_triangles(
            painter,
            &data.road_triangles,
            &self.road_index,
            anchor,
            radius,
            color,
        );
    }
}

fn paint_triangles(
    painter: &mut Painter,
    triangles: &[[[f32; 2]; 3]],
    index: &omsi_proton::navigation::TriangleIndex,
    anchor: DVec2,
    radius: Option<f64>,
    color: Color,
) {
    let ids =
        radius.map(|r| index.nearby(triangles, anchor.to_array().map(|n| n as f32), r as f32));
    if let Some(ids) = ids {
        paint_ids(painter, triangles, anchor, color, ids.into_iter());
    } else {
        paint_ids(painter, triangles, anchor, color, 0..triangles.len());
    }
}

fn paint_ids(
    painter: &mut Painter,
    triangles: &[[[f32; 2]; 3]],
    anchor: DVec2,
    color: Color,
    ids: impl Iterator<Item = usize>,
) {
    for i in ids {
        painter.verts.extend(triangles[i].map(|p| Vertex {
            pos: [
                (p[0] as f64 - anchor.x) as f32,
                (p[1] as f64 - anchor.y) as f32,
                0.,
            ],
            color: color.0,
            mode: [1., 0.],
            ..Default::default()
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
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
        let raw = dir.join("navigation-source");
        std::fs::write(&raw,br#"{"version":1,"routes":[],"roads":[{"name":"main.txt","points":[[10,20,7],[30,40,8]]},{"name":"manual.txt","points":[[0,0,0],[100,0,0]],"overview_only":true}],"road_triangles":[[[0,0],[10,0],[0,10]]]}"#).unwrap();
        omsi_cfg::oi::encode_file(&raw, &file, omsi_cfg::oi::Kind::Navigation).unwrap();
        let world = crate::World::open(&dir, &dir.join("global.cfg"), 20261001).unwrap();
        let map = world.navigation_map();
        assert_eq!(map.lanes.len(), 1);
        assert_eq!(
            map.lanes[0].points,
            vec![DVec3::new(10.0, 20.0, 7.0), DVec3::new(30.0, 40.0, 8.0)]
        );
        assert_eq!(map.lanes[0].density, 0.0);
        assert!(world.lanes.lock().is_empty());
        assert_eq!(map.imported.as_ref().unwrap().road_triangles.len(), 1);
        let (roads, _) = crate::navigation_roads::city_roads(map.lanes, &[]);
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
            road_triangles: vec![],
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

#[cfg(test)]
mod reload_tests {
    use super::*;
    #[test]
    fn replacement_invalidates_selected_route_and_road_buffers_and_manual_links_stay_visual() {
        let make = |x| {
            Arc::new(Navigation {
                version: 1,
                road_triangles: vec![[[x, 0.], [x + 10., 0.], [x, 10.]]],
                routes: vec![omsi_proton::navigation::Route {
                    name: "Centro Bairro".into(),
                    triangles: vec![[[x, 0.], [x + 2., 0.], [x, 2.]]],
                }],
                roads: vec![],
            })
        };
        let mut gps = Gps::default();
        gps.select("CENTRO_BAIRRO");
        gps.set_data(Some(make(0.)));
        let old = gps.cache_key();
        let mut p = Painter::new();
        gps.paint(&mut p, DVec2::ZERO, Some(20.), Color::WHITE);
        assert_eq!(p.len(), 3);
        gps.set_data(Some(make(1000.)));
        assert_ne!(gps.cache_key(), old);
        p.clear();
        gps.paint(&mut p, DVec2::ZERO, Some(20.), Color::WHITE);
        assert!(p.is_empty());
        gps.paint_roads(&mut p, DVec2::new(1000., 0.), Some(20.), Color::WHITE);
        assert_eq!(p.len(), 3);
        assert_eq!(gps.map_extent().unwrap().1, DVec2::new(1010., 10.));
        gps.set_data(None);
        assert!(gps.extent().is_none());
    }
}

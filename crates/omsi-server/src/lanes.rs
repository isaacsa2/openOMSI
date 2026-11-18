//! CPU-only OMSI [path] lane extraction.
//!
//! Geometry is derived from .sli/.sco definitions and map rules; no mesh,
//! material, texture, scene graph, Vulkan, DirectX or audio is allocated.
//! Based on the same lane geometry used by the graphical client to avoid
//! divergences in mirrored splines, one-way streets and timed routes.

use glam::{DVec2, DVec3};
use omsi_geometry::SplineCurve;
use omsi_scenery::{SceneryObject, Spline};
use omsi_sim::traffic::{Lane, LaneBuilder, LaneKey, LaneKind};

/// A path's `[rule] trafficdensity`s: how much random traffic of any group it carries,
/// and the last value per group (the rule's fourth line: the group's place in the map's
/// `unsched_vehgroups.txt`). Without a rule for the first group the path has its medium
/// density (1); the lane carries traffic as long as any group drives on it - on
/// Berlin-Spandau 462 Falkensee paths set only the GDR cars' density.
pub fn path_densities(rules: &[omsi_map::MapRule], path: usize) -> (f32, Vec<(u16, f32)>) {
    let mut per: Vec<(u16, f32)> = Vec::new();
    for r in rules.iter().filter(|r| {
        r.path_index == path as i32 && r.kind.eq_ignore_ascii_case("trafficdensity") && !r.kill
    }) {
        let g = r.extra.max(0.0) as u16;
        let v = (r.value as f32).max(0.0);
        match per.iter_mut().find(|(k, _)| *k == g) {
            Some(e) => e.1 = v,
            None => per.push((g, v)),
        }
    }
    let first = per.iter().find(|(g, _)| *g == 0).map(|e| e.1).unwrap_or(1.0);
    let density = per.iter().map(|e| e.1).fold(first, f32::max);
    (density, per)
}

/// Lanes of one map spline: every `[path]` of the spline type runs along the curve at its
/// lateral offset; `direction` 1 runs backwards, 2 both ways (two lanes).
///
/// A spline placed with `mirror` has its cross-section turned over: each path lies on the
/// other side of the centre line and runs the other way, as its carriageway does (a right
/// lane that ran forward is a left lane running backward - traffic still keeps right).
/// Ignoring the flag put every lane of Spandau's 40-odd mirrored road pieces 5 to 20 m
/// beside its road and the wrong way round; a timetable route through one had its bus turn
/// into the oncoming lanes and jump back where the next piece began.
pub fn spline_lanes(
    def: &Spline,
    s: &omsi_map::MapSpline,
    curve: &SplineCurve,
    tile: (i32, i32),
) -> Vec<Lane> {
    let mut out = Vec::new();
    let side = if s.mirror { -1.0 } else { 1.0 };
    let curve = &curve.with_sli(def);
    for (pi, p) in def.paths.iter().enumerate() {
        let n = ((curve.length / 3.0).ceil() as usize).clamp(1, 300);
        let pts: Vec<DVec3> = (0..=n)
            .map(|i| {
                curve.offset_point(
                    curve.length * i as f64 / n as f64,
                    side * p.start[0] as f64,
                    p.start[2] as f64,
                )
            })
            .collect();
        let kind = LaneKind::from_code(p.kind);
        let limit = s
            .rules
            .iter()
            .filter(|r| {
                r.path_index == pi as i32
                    && r.kind.eq_ignore_ascii_case("speedlimit")
                    && r.value > 0.0
            })
            .map(|r| r.value as f32)
            .last();
        // The other rules of this path: how much traffic the mapper wants here at all.
        // Berlin-Spandau alone carries 8516 [rule] trafficdensity and 68 no_cars, and with
        // them ignored cars appeared in pedestrian streets, depot yards and back lanes the
        // original keeps empty.
        let rule_of = |name: &str| {
            s.rules
                .iter()
                .filter(|r| {
                    r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case(name) && !r.kill
                })
                .map(|r| r.value as f32)
                .last()
        };
        let (density, group_density) = path_densities(&s.rules, pi);
        let no_cars = s.rules.iter().any(|r| {
            r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case("no_cars") && !r.kill
        });
        // (`bus` and `trucks` are switches that open the path to those AI vehicles, see
        // `Lane::allows`; `bus` does not close it to cars)
        let rule_bus = rule_of("bus").is_some();
        let rule_trucks = rule_of("trucks").is_some();
        let no_overtaking = rule_of("overtaking_prohib").is_some();
        let priority = rule_of("priority");
        let mut push = |pts: Vec<DVec3>, reversed: bool| {
            let mut l = LaneBuilder::polyline(pts, kind, p.width);
            if let Some(v) = priority {
                l.priority = v;
            }
            if let Some(v) = limit {
                l.speed_limit_kmh = v;
            }
            l.density = density;
            l.group_density = group_density.clone();
            l.no_cars = no_cars;
            l.rule_bus = rule_bus;
            l.rule_trucks = rule_trucks;
            l.no_overtaking = no_overtaking;
            l.source = 1;
            l.key = Some(LaneKey {
                tile,
                id: s.id,
                path: pi as u16,
            });
            l.reversed = reversed;
            l.offset = side as f32 * p.start[0];
            l.name = def
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(l);
        };
        // (a mirrored spline's forward path runs backwards and the other way round)
        match (p.direction, s.mirror) {
            (2, _) => {
                push(pts.clone(), false);
                push(pts.into_iter().rev().collect(), true);
            }
            (1, false) | (0, true) => push(pts.iter().rev().copied().collect(), true),
            _ => push(pts, false),
        }
    }
    out
}


/// Lanes of one placed scenery object: `[path]` arcs in the object frame (x right,
/// y forward, z up; heading clockwise, radius > 0 right turn) rotated by the object heading.
pub fn object_lanes(
    sco: &SceneryObject,
    pos: DVec3,
    rot: [f64; 3],
    controller: Option<usize>,
    tile: (i32, i32),
    id: i64,
    rules: &[omsi_map::MapRule],
) -> Vec<Lane> {
    let mut out = Vec::new();
    let heading = rot[0];
    let h = heading.to_radians();
    let (sh, ch) = (h.sin(), h.cos());
    // local (x east, y north) → world for an object turned clockwise by `heading`
    let to_world = |x: f64, y: f64| DVec2::new(x * ch + y * sh, -x * sh + y * ch);
    for (pi, p) in sco.paths.iter().enumerate() {
        let v = &p.params;
        if v.len() < 11 {
            continue;
        }
        let start_local = to_world(v[0] as f64, v[1] as f64);
        let start = DVec3::new(
            pos.x + start_local.x,
            pos.y + start_local.y,
            pos.z + v[2] as f64,
        );
        let (path_heading, radius, length) = (v[3] as f64 + heading, v[4] as f64, v[5] as f64);
        if length <= 0.01 {
            continue;
        }
        let (grad_start, grad_end) = (v[6] as f64, v[7] as f64);
        let kind = LaneKind::from_code(p.kind);
        let turn = match v.get(11).map(|t| *t as i32) {
            Some(2) => 1,
            Some(3) => 2,
            _ => 0,
        };
        // The `[rule]`s the map put on this object's path. Most of a map's rules sit on the
        // junctions, not on the splines - Berlin-Spandau has 5402 trafficdensity, 693
        // speedlimit, 576 trucks and 64 no_cars on objects against 3114/810/284/4 on splines
        // - so ignoring them left cars turning into every yard and pedestrian street.
        let rule_of = |name: &str| {
            rules
                .iter()
                .filter(|r| {
                    r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case(name) && !r.kill
                })
                .map(|r| r.value as f32)
                .last()
        };
        let limit = rules
            .iter()
            .filter(|r| {
                r.path_index == pi as i32
                    && r.kind.eq_ignore_ascii_case("speedlimit")
                    && r.value > 0.0
                    && !r.kill
            })
            .map(|r| r.value as f32)
            .last();
        let (density, group_density) = path_densities(rules, pi);
        let no_cars = rules.iter().any(|r| {
            r.path_index == pi as i32
                && r.kind.eq_ignore_ascii_case("no_cars")
                && !r.kill
        });
        let rule_bus = rule_of("bus").is_some();
        let rule_trucks = rule_of("trucks").is_some();
        let no_overtaking = rule_of("overtaking_prohib").is_some();
        // who goes first where this path meets another (`Network::must_yield`)
        let priority = rule_of("priority");
        let mut push = |reverse: bool| {
            let mut l = LaneBuilder::arc_with_gradients(start, path_heading, length, radius, grad_start, grad_end, kind, p.width);
            if reverse {
                let pts: Vec<DVec3> = l.points.iter().rev().copied().collect();
                l = LaneBuilder::polyline(pts, kind, p.width);
            }
            if let Some(v) = limit {
                l.speed_limit_kmh = v;
            }
            l.density = density;
            l.group_density = group_density.clone();
            l.no_cars = no_cars;
            l.rule_bus = rule_bus;
            l.rule_trucks = rule_trucks;
            l.no_overtaking = no_overtaking;
            l.turn = turn;
            if let Some(v) = priority {
                l.priority = v;
            }
            l.source = 2;
            l.key = Some(LaneKey {
                tile,
                id,
                path: pi as u16,
            });
            l.blocks = sco.path_blocks.get(pi).map(|b| b.iter().filter(|(n, _)| *n >= 0).map(|(n, _)| *n as u16).collect()).unwrap_or_default();
            l.crossing_problem = sco.path_crossing_problem.get(pi).copied().unwrap_or(false);
            l.reversed = reverse;
            l.traffic_light = controller.and_then(|c| {
                sco.path_traffic_light
                    .get(pi)
                    .copied()
                    .filter(|t| *t >= 0)
                    .map(|t| (c, t as usize))
            });
            out.push(l);
        };
        match p.direction {
            1 => push(true),
            2 => {
                push(false);
                push(true);
            }
            _ => push(false),
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_group_rules_match_map_semantics() {
        let r = |group: f64, density: f64| omsi_map::MapRule {
            path_index: 1, kind: "trafficdensity".into(), value: density,
            extra: group, ..Default::default()
        };
        let (density, groups) = path_densities(&[r(0.0, 0.5), r(3.0, 0.1), r(0.0, 0.7)], 1);
        assert!((density - 0.7).abs() < 0.0001);
        assert_eq!(groups, vec![(0, 0.7), (3, 0.1)]);
        assert_eq!(path_densities(&[r(0.0, 0.5)], 0), (1.0, vec![]));
    }
}

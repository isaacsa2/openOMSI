//! Sharp corners and holes in authored vehicle paths that the native traffic network cannot
//! drive through.
//!
//! The native network joins a lane to the next one only when the end of the first lies within
//! 1.5 m of the start of the second and its heading changes by less than 40 degrees. A Proton
//! chain is a polyline: a street corner drawn as one point (or as two chains that meet at a
//! right angle) is a heading change of 90 degrees, so a car reaching it has no next lane and is
//! taken off the road in plain view. The corner is rounded here, and two chains that meet
//! within the network's own tolerance are joined by a short curve, so the street goes on where
//! the author's lines meet. Nothing is added where the author's lines do not meet: a chain
//! that ends in the open stays an end, and a hairpin stays a U-turn.
//!
//! Nothing that already joins is touched: an end that has a native link keeps its place, and a
//! corner where another chain begins or ends is left as drawn, so a repair can never cut a
//! junction that worked.
use crate::{paths::DrivePath, paths::Point, Report};
use glam::{Vec2, Vec3};

/// The native tolerance between the end of a lane and the start of the next (m).
const LINK: f32 = 1.5;
/// The native limit of a heading change between two joined lanes (degrees).
const NATIVE_TURN: f32 = 40.0;
/// From this heading change a corner inside a chain is rounded: just under the native limit,
/// so a corner the network can barely take is not left as it is.
const ROUND_FROM: f32 = 36.0;
/// A heading change beyond this is a turn-back the author drew on purpose.
const ROUND_TO: f32 = 115.0;
/// The radius a corner is rounded with (m), as far as its two lanes are long enough.
const RADIUS: f32 = 8.0;
/// Each curve lane turns at most this much (degrees).
const STEP_TURN: f32 = 20.0;
/// A hole this wide between a dead end and a compatible start is closed (m).
const BRIDGE: f32 = 6.0;
/// A lane never gives up more than this share of its length to a curve: two curves on one lane
/// can then never overlap.
const SHARE: f32 = 0.45;
/// Shorter than this a connector collapses (the exported lanes drop pieces under 1 cm, and a
/// pivot in place is no curve).
const MIN_CHORD: f32 = 0.3;

fn horizontal(a: Vec3, b: Vec3) -> Option<Vec2> {
    let d = (b - a).truncate();
    (d.length() > 0.01).then(|| d.normalize())
}

fn turn(a: Vec2, b: Vec2) -> f32 {
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The points of a cubic curve from `a` to `b` leaving along `da` and arriving along `db`,
/// without the two ends. Height runs straight from one end to the other.
fn curve(a: Vec3, b: Vec3, da: Vec2, db: Vec2, theta: f32) -> Vec<Vec3> {
    let steps = (theta / STEP_TURN).ceil().max(1.0) as usize + 1;
    let handle = 0.4 * (b - a).truncate().length();
    let p1 = a + da.extend(0.0) * handle;
    let p2 = b - db.extend(0.0) * handle;
    (1..steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            let u = 1.0 - t;
            let mut p =
                a * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + b * (t * t * t);
            p.z = a.z + (b.z - a.z) * t;
            p
        })
        .collect()
}

fn with_pos(template: &Point, pos: Vec3, density: f32) -> Point {
    let mut p = template.clone();
    p.pos = pos.to_array();
    p.density = density;
    p
}

fn ideal_cut(theta: f32) -> f32 {
    RADIUS * (theta.to_radians() / 2.0).tan()
}

/// One end of a chain: where it stops, the heading it arrives (or leaves) with and how long
/// its last (or first) lane is.
#[derive(Clone, Copy)]
struct Tip {
    chain: usize,
    pos: Vec3,
    /// The neighbouring point of its chain: the one before an end, the one after a start.
    other: Vec3,
    dir: Vec2,
    len: f32,
}

fn tips(paths: &[DrivePath]) -> (Vec<Tip>, Vec<Tip>) {
    let (mut ends, mut starts) = (Vec::new(), Vec::new());
    for (chain, path) in paths.iter().enumerate() {
        let p = &path.points;
        if path.pedestrian || p.len() < 2 || p[0].pos == p[p.len() - 1].pos {
            continue;
        }
        let pos = |i: usize| Vec3::from(p[i].pos);
        let n = p.len();
        if let Some(dir) = horizontal(pos(n - 2), pos(n - 1)) {
            ends.push(Tip {
                chain,
                pos: pos(n - 1),
                other: pos(n - 2),
                dir,
                len: (pos(n - 1) - pos(n - 2)).truncate().length(),
            });
        }
        if let Some(dir) = horizontal(pos(0), pos(1)) {
            starts.push(Tip {
                chain,
                pos: pos(0),
                other: pos(1),
                dir,
                len: (pos(1) - pos(0)).truncate().length(),
            });
        }
    }
    (ends, starts)
}

fn gap(a: &Tip, b: &Tip) -> f32 {
    (a.pos - b.pos).truncate().length()
}

/// Does the native network join this end to this start?
fn native_link(end: &Tip, start: &Tip) -> bool {
    end.chain != start.chain
        && gap(end, start) <= LINK
        && (end.pos.z - start.pos.z).abs() < 3.0
        && turn(end.dir, start.dir) < NATIVE_TURN
}

/// Round every sharp interior corner of a chain: the corner point becomes a curve that starts
/// and ends on the two lanes it joined. The curve keeps the corner's rules (blinker, speed); a
/// corner that was a place cars appear keeps that at the end of the curve, where its lane
/// begins. A corner where another chain begins or ends stays as drawn: moving the lane starts
/// away from it would cut that chain off.
fn round_chain(
    points: Vec<Point>,
    chain: usize,
    others: &[(usize, Vec3)],
    report: &mut Report,
) -> Vec<Point> {
    let n = points.len();
    if n < 3 || points[0].pos == points[n - 1].pos {
        return points;
    }
    let at = |i: usize| Vec3::from(points[i].pos);
    let mut out = Vec::with_capacity(n);
    out.push(points[0].clone());
    for (i, point) in points.iter().enumerate().take(n - 1).skip(1) {
        let (before, corner, after) = (at(i - 1), at(i), at(i + 1));
        let attached = others.iter().any(|&(c, p)| {
            c != chain && (p - corner).truncate().length() <= LINK && (p.z - corner.z).abs() < 3.0
        });
        let (Some(d1), Some(d2)) = (horizontal(before, corner), horizontal(corner, after)) else {
            out.push(point.clone());
            continue;
        };
        let theta = turn(d1, d2);
        if attached || !(ROUND_FROM..=ROUND_TO).contains(&theta) {
            out.push(point.clone());
            continue;
        }
        let (l1, l2) = (
            (corner - before).truncate().length(),
            (after - corner).truncate().length(),
        );
        let cut = ideal_cut(theta).min(SHARE * l1).min(SHARE * l2);
        let start = before.lerp(corner, 1.0 - cut / l1);
        let end = corner.lerp(after, cut / l2);
        out.push(with_pos(point, start, 0.0));
        for p in curve(start, end, d1, d2, theta) {
            out.push(with_pos(point, p, 0.0));
        }
        out.push(with_pos(point, end, point.density));
        report.road_corners_rounded += 1;
    }
    out.push(points[n - 1].clone());
    out
}

/// Join chains that meet at a sharp angle and close small holes between a dead end and a
/// compatible start. A chain is only ever shortened at a joint that had no native link (by at
/// most a share of its last or first lane) and short curve chains are added; no other point
/// moves.
pub(crate) fn connect(paths: &mut [DrivePath], report: &mut Report) -> Vec<DrivePath> {
    let (ends, starts) = tips(paths);
    let attached: Vec<(usize, Vec3)> = ends
        .iter()
        .chain(&starts)
        .map(|t| (t.chain, t.pos))
        .collect();
    for (chain, path) in paths.iter_mut().enumerate().filter(|(_, p)| !p.pedestrian) {
        let points = std::mem::take(&mut path.points);
        path.points = round_chain(points, chain, &attached, report);
    }
    // the ends and starts, after the corners inside the chains have been rounded: a rounded
    // corner next to a tip shortens that tip's lane but keeps its heading
    let (ends, starts) = tips(paths);
    let end_native: Vec<bool> = ends
        .iter()
        .map(|e| starts.iter().any(|s| native_link(e, s)))
        .collect();
    let start_native: Vec<bool> = starts
        .iter()
        .map(|s| ends.iter().any(|e| native_link(e, s)))
        .collect();
    // sharp joints: an end and a start within the native tolerance that the native rule rejects
    let mut sharp: Vec<(usize, usize, f32)> = Vec::new();
    for (e, end) in ends.iter().enumerate() {
        for (s, start) in starts.iter().enumerate() {
            if end.chain == start.chain
                || gap(end, start) > LINK
                || (end.pos.z - start.pos.z).abs() >= 3.0
            {
                continue;
            }
            let theta = turn(end.dir, start.dir);
            if (NATIVE_TURN..=ROUND_TO).contains(&theta) {
                sharp.push((e, s, theta));
            }
        }
    }
    // how much each end and each start gives up; one that already has another native link
    // gives nothing, or that link would be cut
    let mut give_end = vec![f32::INFINITY; ends.len()];
    let mut give_start = vec![f32::INFINITY; starts.len()];
    for &(e, s, theta) in &sharp {
        let ideal = ideal_cut(theta);
        let ce = if end_native[e] {
            0.0
        } else {
            ideal.min(SHARE * ends[e].len)
        };
        let cs = if start_native[s] {
            0.0
        } else {
            ideal.min(SHARE * starts[s].len)
        };
        let (ge, gs) = if ce > 0.0 && cs > 0.0 {
            (ce.min(cs), ce.min(cs))
        } else {
            (ce, cs)
        };
        if ge > 0.0 {
            give_end[e] = give_end[e].min(ge);
        }
        if gs > 0.0 {
            give_start[s] = give_start[s].min(gs);
        }
    }
    let mut new_end = vec![None; ends.len()];
    let mut new_start = vec![None; starts.len()];
    for (e, end) in ends
        .iter()
        .enumerate()
        .filter(|(e, _)| give_end[*e].is_finite())
    {
        let path = &mut paths[end.chain].points;
        let last = path.len() - 1;
        let back = end.other.lerp(end.pos, 1.0 - give_end[e] / end.len);
        path[last].pos = back.to_array();
        new_end[e] = Some(back);
    }
    for (s, start) in starts
        .iter()
        .enumerate()
        .filter(|(s, _)| give_start[*s].is_finite())
    {
        let path = &mut paths[start.chain].points;
        let ahead = start.pos.lerp(start.other, give_start[s] / start.len);
        path[0].pos = ahead.to_array();
        new_start[s] = Some(ahead);
    }
    let mut connectors = Vec::new();
    let (mut handled_end, mut handled_start) = (vec![false; ends.len()], vec![false; starts.len()]);
    for &(e, s, theta) in &sharp {
        handled_end[e] = true;
        handled_start[s] = true;
        let a = new_end[e].unwrap_or(ends[e].pos);
        let b = new_start[s].unwrap_or(starts[s].pos);
        if (b - a).truncate().length() < MIN_CHORD {
            continue;
        }
        let template = paths[ends[e].chain]
            .points
            .last()
            .expect("a chain has points")
            .clone();
        let rule = paths[starts[s].chain].points[0].clone();
        let mut points = vec![with_pos(&template, a, 0.0)];
        for p in curve(a, b, ends[e].dir, starts[s].dir, theta) {
            points.push(with_pos(&template, p, 0.0));
        }
        points.push(with_pos(&template, b, 0.0));
        for p in &mut points {
            p.turn = rule.turn.max(template.turn);
        }
        connectors.push(DrivePath {
            points,
            pedestrian: false,
        });
        report.road_joints_connected += 1;
    }
    // holes: a dead end and an orphan start, a little too far apart for the native tolerance
    let mut taken = vec![false; starts.len()];
    for (_, end) in ends
        .iter()
        .enumerate()
        .filter(|(e, _)| !end_native[*e] && !handled_end[*e])
    {
        let best = starts
            .iter()
            .enumerate()
            .filter(|(s, start)| {
                !start_native[*s]
                    && !handled_start[*s]
                    && !taken[*s]
                    && start.chain != end.chain
                    && gap(end, start) > LINK
                    && gap(end, start) <= BRIDGE
            })
            .filter_map(|(s, start)| {
                let way = horizontal(end.pos, start.pos)?;
                let ok = turn(end.dir, way) < ROUND_FROM
                    && turn(way, start.dir) < ROUND_FROM
                    && (end.pos.z - start.pos.z).abs() < 3.0;
                ok.then_some((gap(end, start), s))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, s)) = best else { continue };
        taken[s] = true;
        let template = paths[end.chain]
            .points
            .last()
            .expect("a chain has points")
            .clone();
        connectors.push(DrivePath {
            points: vec![
                with_pos(&template, end.pos, 0.0),
                with_pos(&template, starts[s].pos, 0.0),
            ],
            pedestrian: false,
        });
        report.road_gaps_bridged += 1;
    }
    open_ends(paths, &connectors, report);
    connectors
}

/// What is left after the repairs: ends of vehicle paths the native network cannot continue.
/// A car that reaches one is taken off the road there, so they are counted and the first few
/// are named, for the author to see whether the street goes on past them.
fn open_ends(paths: &[DrivePath], connectors: &[DrivePath], report: &mut Report) {
    let all: Vec<DrivePath> = paths.iter().chain(connectors).cloned().collect();
    let (ends, starts) = tips(&all);
    let mut open: Vec<(bool, Vec3)> = Vec::new();
    for end in &ends {
        if starts.iter().any(|s| native_link(end, s)) {
            continue;
        }
        let near = starts
            .iter()
            .any(|s| s.chain != end.chain && gap(end, s) <= 15.0);
        open.push((near, end.pos));
    }
    report.road_ends_open = open.len();
    if open.is_empty() {
        return;
    }
    let near = open.iter().filter(|o| o.0).count();
    // the ones with another path starting close by first: those are where a street may go on
    open.sort_by_key(|o| !o.0);
    let named: Vec<String> = open
        .iter()
        .take(4)
        .map(|(_, p)| format!("({:.0}, {:.0})", p.x, p.y))
        .collect();
    report.warnings.push(format!(
        "{} vehicle path ends have no continuation the native traffic network can join ({near} with another path starting within 15 m, {} in the open): cars are taken off the road there; first at {}",
        open.len(),
        open.len() - near,
        named.join(", ")
    ));
}

#[cfg(test)]
#[path = "path_corners_tests.rs"]
mod tests;

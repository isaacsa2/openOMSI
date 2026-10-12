//! Shared road footprints and street-name indexing for map/navigation presentation.
use glam::{DVec2,DVec3,Vec3};
use hashbrown::HashMap;
use omsi_sim::traffic::{LaneKey,LaneKind,Network};

/// Street lanes that stand for drawn roads, including editor-only paths corroborated by
/// road surfaces. Uncorroborated helpers must not make a phantom road on the GPS.
pub fn visible_road_lanes(net: &Network) -> Vec<(usize, &omsi_sim::traffic::Lane)> {
    let mut seen = hashbrown::HashSet::<(LaneKey, u32)>::new();
    net.lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2)
        .filter(|(_, l)| {
            if let Some(key) = l.key {
                // A two-way [path] has the same geometry in both directions. Keep one
                // casing and surface; retain separate [path]s, which may be real lanes.
                return seen.insert((key, l.source));
            }
            true
        })
        .collect()
}

/// The roads a city map draws, from a whole map's lanes as they were read off the tiles:
/// every path linked to its neighbours, the editor-only ones confirmed by the map's own
/// asphalt, then grouped into carriageways with their real widths. The navigator's city map
/// and the launcher's map picture draw the same roads this way, so the two cannot drift
/// apart; the second value is the linked network the caller may walk (a trip's own lanes).
pub fn city_roads(lanes: Vec<omsi_sim::traffic::Lane>, surfaces: &[(Vec<DVec3>, f32)]) -> (Vec<MapRoad>, Network) {
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    confirm_road_surfaces(&mut net, surfaces);
    let roads = road_geometry(&net);
    (roads, net)
}

/// A road of the city map: the centre line of a carriageway, as wide as the map makes it.
pub struct MapRoad {
    pub points: Vec<DVec3>,
    pub width: f32,
    /// A road cars drive far along (the map's own speed limits say so): drawn brighter.
    pub main: bool,
}

/// Some maps separate the asphalt mesh from their editor-only traffic splines. Use the
/// actual road footprint to distinguish those paths from invisible scenery helpers. This
/// changes only the navigator's copy of the network; driving still uses the original data.
pub fn confirm_road_surfaces(net: &mut Network, surfaces: &[(Vec<DVec3>, f32)]) {
    let mut segments = Vec::new();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (pts, width) in surfaces {
        for ab in pts.windows(2) {
            let (a, b) = (ab[0], ab[1]);
            let margin = *width as f64 * 0.5 + 0.5;
            let lo = a.truncate().min(b.truncate()) - DVec2::splat(margin);
            let hi = a.truncate().max(b.truncate()) + DVec2::splat(margin);
            let (x0, y0) = Network::grid_cell(lo.extend(0.0));
            let (x1, y1) = Network::grid_cell(hi.extend(0.0));
            let i = segments.len();
            segments.push((a, b, margin));
            for x in x0..=x1 { for y in y0..=y1 { grid.entry((x, y)).or_default().push(i); } }
        }
    }
    for lane in net.lanes.iter_mut().filter(|l| l.invisible && l.kind == LaneKind::Street) {
        let n = (lane.length() / 8.0).ceil().max(1.0) as usize;
        let mut covered = 0;
        for k in 0..n {
            let (p, _) = lane.at(lane.length() * (k as f32 + 0.5) / n as f32);
            let on_surface = grid.get(&Network::grid_cell(p)).map(|ids| ids.iter().any(|&i| {
                let (a, b, margin) = segments[i];
                let ab = (b - a).truncate();
                let t = ((p - a).truncate().dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let q = a.lerp(b, t);
                (p - q).truncate().length() <= margin && (p.z - q.z).abs() <= 2.0
            })).unwrap_or(false);
            if on_surface { covered += 1; }
        }
        if covered * 4 >= n * 3 { lane.invisible = false; }
    }
    // Crossings made of objects can have no asphalt spline. Keep helpers joining two
    // corroborated roads; an isolated invisible helper still does not become a street.
    let mut prev = vec![Vec::new(); net.lanes.len()];
    for (i, lane) in net.lanes.iter().enumerate().filter(|(_, l)| l.kind == LaneKind::Street) {
        for &j in &lane.next {
            if net.lanes.get(j).map(|l| l.kind == LaneKind::Street).unwrap_or(false) { prev[j].push(i); }
        }
    }
    let mut seen = vec![false; net.lanes.len()];
    let mut keep = Vec::new();
    for seed in 0..net.lanes.len() {
        if seen[seed] || !net.lanes[seed].invisible || net.lanes[seed].kind != LaneKind::Street { continue; }
        let mut queue = vec![seed];
        let mut component = Vec::new();
        let mut anchors = hashbrown::HashSet::new();
        while let Some(i) = queue.pop() {
            if seen[i] { continue; }
            seen[i] = true;
            component.push(i);
            for &j in net.lanes[i].next.iter().chain(prev[i].iter()) {
                let Some(l) = net.lanes.get(j).filter(|l| l.kind == LaneKind::Street) else { continue };
                if l.invisible {
                    if !seen[j] { queue.push(j); }
                } else {
                    anchors.insert((l.key, l.source, if l.key.is_none() { j } else { 0 }));
                }
            }
        }
        if anchors.len() >= 2 { keep.extend(component); }
    }
    for i in keep { net.lanes[i].invisible = false; }
}

/// Traffic paths are lane centres, not separate streets. Adjacent paths on one spline
/// describe one carriageway; combine their footprints without filling a median. Align
/// opposing directions before averaging so bends, mirrors and ramps retain their shape.
pub fn road_geometry(net: &Network) -> Vec<MapRoad> {
    let mut roads = Vec::new();
    let mut splines = std::collections::BTreeMap::<((i32, i32), i64), Vec<&omsi_sim::traffic::Lane>>::new();
    for (_, lane) in visible_road_lanes(net) {
        if let Some(key) = lane.key.filter(|_| lane.source == 1) {
            splines.entry((key.tile, key.id)).or_default().push(lane);
        } else {
            roads.push(MapRoad { points: lane.points.clone(), width: lane.width.max(2.6), main: lane.speed_limit_kmh >= 55.0 });
        }
    }
    for mut lanes in splines.into_values() {
        lanes.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let mut start = 0;
        while start < lanes.len() {
            let a = lanes[start];
            let lo = a.offset - a.width.max(2.6) * 0.5;
            let mut hi = a.offset + a.width.max(2.6) * 0.5;
            let mut end = start + 1;
            while end < lanes.len() {
                let b = lanes[end];
                if b.points.len() != a.points.len() || b.offset - b.width.max(2.6) * 0.5 > hi + 0.65 { break; }
                let mid = a.points.len() / 2;
                let za = a.points[if a.reversed { a.points.len() - 1 - mid } else { mid }].z;
                let zb = b.points[if b.reversed { b.points.len() - 1 - mid } else { mid }].z;
                if (za - zb).abs() > 1.5 { break; }
                hi = hi.max(b.offset + b.width.max(2.6) * 0.5);
                end += 1;
            }
            let b = lanes[end - 1];
            let span = b.offset - a.offset;
            let t = if span.abs() > 0.01 { (((lo + hi) * 0.5 - a.offset) / span) as f64 } else { 0.0 };
            let point = |l: &omsi_sim::traffic::Lane, i: usize| l.points[if l.reversed { l.points.len() - 1 - i } else { i }];
            let points = (0..a.points.len()).map(|i| point(a, i).lerp(point(b, i), t)).collect();
            roads.push(MapRoad { points, width: hi - lo, main: lanes[start..end].iter().any(|l| l.speed_limit_kmh >= 55.0) });
            start = end;
        }
    }
    // Bridge only explicit graph connections within the map's placement tolerance.
    for lane in net.lanes.iter().filter(|l| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2) {
        for &j in &lane.next {
            let Some(next) = net.lanes.get(j).filter(|l| l.kind == LaneKind::Street && !l.invisible && l.points.len() >= 2) else { continue };
            let distance = lane.end().distance(next.start());
            if distance > 0.05 && distance <= 2.0 {
                roads.push(MapRoad { points: vec![lane.end(), next.start()], width: lane.width.min(next.width).max(2.6), main: lane.speed_limit_kmh >= 55.0 && next.speed_limit_kmh >= 55.0 });
            }
        }
    }
    roads
}

/// `pts` with the points dropped that lie within `tol` metres of the line through their
/// neighbours kept (Douglas-Peucker): a lane is sampled every metre or two, a ribbon needs
/// only its bends. The launcher's map picture draws its roads with this too.
pub fn simplify(pts: &[Vec3], tol: f32) -> Vec<Vec3> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a].truncate(), pts[b].truncate());
        let ab = pb - pa;
        let len = ab.length().max(1e-6);
        let mut worst = (0.0f32, 0usize);
        for k in a + 1..b {
            let d = (pts[k].truncate() - pa).perp_dot(ab).abs() / len;
            if d > worst.0 {
                worst = (d, k);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Lanes with a point within `radius` of `c` (by the network's 50 m grid).
pub fn lanes_near(net: &Network, c: DVec2, radius: f64) -> Vec<usize> {
    let mut seen = hashbrown::HashSet::new();
    let (cx, cy) = Network::grid_cell(c.extend(0.0));
    let r = (radius / 50.0).ceil() as i32;
    if net.grid.is_empty() {
        return (0..net.lanes.len()).filter(|&i| net.lanes[i].points.iter().any(|q| (q.truncate() - c).length() < radius)).collect();
    }
    for gx in cx - r..=cx + r {
        for gy in cy - r..=cy + r {
            if let Some(v) = net.grid.get(&(gx, gy)) {
                seen.extend(v.iter().copied());
            }
        }
    }
    let mut v: Vec<usize> = seen.into_iter().collect();
    v.sort_unstable();
    v
}

/// `OMSI_NAV_PROBE=x,y[,r]`: the lanes of the map's network that start or end within r
/// metres (25) of a point - how they link, to see why a route cannot reach a place.
pub struct Streets {
    pub names: Vec<String>,
    /// Per lane: index into `names`, or `u32::MAX`.
    pub of_lane: Vec<u32>,
    /// A place to write a name: the point, the road's direction (radians, world, from +x
    /// anticlockwise) and the name.
    pub labels: Vec<(DVec2, f32, u32)>,
}

/// The heading of a sign whose plate runs along its road, relative to the road (degrees;
/// settled on the stock Verkehrszeichen_MC signs of Berlin-Spandau, see `build_streets`).
pub const SIGN_ALONG: f64 = 90.0;

pub fn build_streets(net: &Network, signs: &[(DVec3, f64, String)]) -> Streets {
    let t0 = std::time::Instant::now();
    let n = net.lanes.len();
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<String, u32> = HashMap::new();
    let mut of_lane = vec![u32::MAX; n];
    // lanes into each lane, to carry names backwards
    let mut prev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, l) in net.lanes.iter().enumerate() {
        for &j in &l.next {
            if j < n {
                prev[j].push(i);
            }
        }
    }
    let straight = |l: &omsi_sim::traffic::Lane| l.kind == LaneKind::Street && omsi_sim::traffic::wrap_deg(l.end_heading() - l.start_heading()).abs() < 30.0;
    let debug = omsi_cfg::flags::OMSI_DEBUG_NAV.is_set();
    let mut hist = [0u32; 12];
    let mut seeds: Vec<(usize, u32)> = Vec::new();
    for (pos, rot, name) in signs {
        let id = *index.entry(name.clone()).or_insert_with(|| {
            names.push(name.clone());
            (names.len() - 1) as u32
        });
        // the straight street lanes near the sign, and how well each runs along its plate
        let mut best: Option<(usize, f64)> = None;
        for i in lanes_near(net, pos.truncate(), 30.0) {
            let l = &net.lanes[i];
            if !straight(l) || l.length() < 12.0 {
                continue;
            }
            let Some((s, d)) = l.nearest_point(DVec3::new(pos.x, pos.y, l.start().z)) else { continue };
            if d > 22.0 {
                continue;
            }
            let (_, h) = l.at(s);
            // (the direction of a road does not matter: modulo 180)
            let off = (angle_diff(*rot, h as f64).abs() - SIGN_ALONG).abs();
            let off = off.min(180.0 - off);
            if debug && d < 12.0 {
                let raw = angle_diff(*rot, h as f64).rem_euclid(180.0);
                hist[((raw / 15.0) as usize).min(11)] += 1;
            }
            let score = d + off * 0.5;
            if off < 35.0 && best.map(|b| score < b.1).unwrap_or(true) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            seeds.push((i, id));
        }
    }
    if debug {
        if let Some((pos, rot, name)) = signs.first() {
            let near = lanes_near(net, pos.truncate(), 30.0);
            let best = near.iter().filter_map(|&i| net.lanes[i].nearest_point(*pos).map(|p| (i, p.1, net.lanes[i].kind, net.lanes[i].length()))).min_by(|a, b| a.1.total_cmp(&b.1));
            log::info!("navigator: first sign '{name}' at {pos:?} heading {rot}: {} lanes near, nearest {best:?}", near.len());
        }
        // which way a plate runs: two signs of one name far apart lie along their street
        let mut along = [0u32; 12];
        for (i, (p, r, n)) in signs.iter().enumerate() {
            for (q, _, m) in &signs[i + 1..] {
                let d = (*q - *p).truncate();
                if n == m && d.length() > 150.0 && d.length() < 700.0 {
                    let h = d.x.atan2(d.y).to_degrees();
                    let raw = angle_diff(*r, h).rem_euclid(180.0);
                    along[((raw / 15.0) as usize).min(11)] += 1;
                }
            }
        }
        log::info!("navigator: sign heading minus the line to another sign of its name (mod 180): {along:?}");
        log::info!("navigator: sign heading minus road heading (mod 180, 15-degree bins): {hist:?}");
    }
    // carry each name along its road: the seed lane, its other direction, and on through
    // straight continuations for up to 1.5 km each way
    for &(seed, id) in &seeds {
        if of_lane[seed] != u32::MAX {
            continue;
        }
        let mut queue = vec![(seed, 0.0f32)];
        while let Some((i, far)) = queue.pop() {
            if of_lane[i] != u32::MAX && i != seed {
                continue;
            }
            of_lane[i] = id;
            let l = &net.lanes[i];
            if let Some(k) = l.key {
                for &j in net.by_key.get(&k).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if of_lane[j] == u32::MAX {
                        of_lane[j] = id;
                    }
                }
            }
            if far > 1500.0 {
                continue;
            }
            for &j in l.next.iter() {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(m.start_heading() - l.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
            for &j in &prev[i] {
                let m = &net.lanes[j];
                if of_lane[j] == u32::MAX && straight(m) && omsi_sim::traffic::wrap_deg(l.start_heading() - m.end_heading()).abs() < 20.0 {
                    queue.push((j, far + m.length()));
                }
            }
        }
    }
    // where to write them: the middle of long named lanes, a name every 350 m at most
    let mut order: Vec<usize> = (0..n).filter(|&i| of_lane[i] != u32::MAX && !net.lanes[i].reversed && net.lanes[i].length() > 30.0).collect();
    order.sort_by(|a, b| net.lanes[*b].length().total_cmp(&net.lanes[*a].length()));
    let mut labels: Vec<(DVec2, f32, u32)> = Vec::new();
    for i in order {
        let l = &net.lanes[i];
        let (q, h) = l.at(l.length() * 0.5);
        let q = q.truncate();
        if labels.iter().any(|(p, _, id)| *id == of_lane[i] && (*p - q).length() < 350.0) {
            continue;
        }
        let hr = (h as f64).to_radians();
        labels.push((q, (hr.cos()).atan2(hr.sin()) as f32, of_lane[i]));
    }
    let named = of_lane.iter().filter(|&&x| x != u32::MAX).count();
    log::info!("navigator: {} street name signs, {} names, {} of {} lanes named, {} labels, {:.0} ms", signs.len(), names.len(), named, n, labels.len(), t0.elapsed().as_secs_f64() * 1000.0);
    Streets { names, of_lane, labels }
}


fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

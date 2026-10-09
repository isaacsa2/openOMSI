//! OMSI_AUTOPILOT=<km/h>: the player's bus drives itself along the road network's lanes at
//! that speed. Offscreen it drives a map's roundabouts and bends to see where the bus falls
//! through or leaves the road; in the window it drives the bus for a measured run
//! (OMSI_PROFILE_JSON with --exit-after) without anybody at the wheel.

use glam::DVec3;
use omsi_sim::collision::Obb;
use omsi_sim::traffic::{LaneKind, Network};
use omsi_sim::VehicleInstance;

/// How far ahead (m) the wheel aims along the lanes.
const AIM_AHEAD: f32 = 12.0;

/// The lanes the bus takes from where it is: the one it is on and the way on, where the road
/// branches the straightest. (lane, metres from the bus to its start, 0 for the first, and
/// where along it the bus is), as far as `reach` metres. Empty: no street lane near.
pub(crate) fn path_ahead(net: &Network, position: DVec3, heading: f64, reach: f32) -> Vec<(usize, f32, f32)> {
    let h = heading.to_radians();
    let fwd = DVec3::new(h.sin(), h.cos(), 0.0);
    let probe = position + fwd * 3.0;
    let Some((mut lane, mut s, _)) = net.nearest_lane(probe, LaneKind::Street) else { return Vec::new() };
    // (the lane that runs our way)
    let lh = net.lanes[lane].at(s).1 as f64;
    let dh = (lh - heading + 540.0).rem_euclid(360.0) - 180.0;
    if dh.abs() > 100.0 {
        if let Some((l2, s2, _)) = (0..net.lanes.len())
            .filter(|&k| net.lanes[k].kind == LaneKind::Street)
            .filter_map(|k| net.lanes[k].nearest_point(probe).map(|(s, d)| (k, s, d)))
            .filter(|(k, s, d)| *d < 6.0 && ((net.lanes[*k].at(*s).1 as f64 - heading + 540.0).rem_euclid(360.0) - 180.0).abs() < 80.0)
            .min_by(|a, b| a.2.total_cmp(&b.2))
        {
            lane = l2;
            s = s2;
        }
    }
    let mut path = vec![(lane, 0.0, s)];
    let mut to_start = -s;
    while to_start + net.lanes[lane].length() < reach && path.len() < 64 {
        let here = net.lanes[lane].at(net.lanes[lane].length()).1;
        let Some(&next) = net.lanes[lane].next.iter().min_by(|a, b| {
            let da = (net.lanes[**a].at(0.0).1 - here + 540.0).rem_euclid(360.0) - 180.0;
            let db = (net.lanes[**b].at(0.0).1 - here + 540.0).rem_euclid(360.0) - 180.0;
            da.abs().total_cmp(&db.abs())
        }) else {
            break;
        };
        to_start += net.lanes[lane].length();
        lane = next;
        path.push((lane, to_start, 0.0));
    }
    path
}

/// Wheel, throttle and brake that keep `v` on `path` (see [`path_ahead`]) at `kmh`: the
/// steering on a pure-pursuit point 12 m ahead, the pedals on the speed.
pub(crate) fn path_controls(net: &Network, path: &[(usize, f32, f32)], v: &VehicleInstance, kmh: f32) -> Option<(f32, f32, f32)> {
    let &(first, _, s0) = path.first()?;
    // (the lane the aim point is on, and where along it)
    let (lane, s) = path
        .iter()
        .rev()
        .find(|(_, to_start, _)| *to_start <= AIM_AHEAD)
        .map(|&(l, to_start, _)| if l == first { (l, s0 + AIM_AHEAD) } else { (l, AIM_AHEAD - to_start) })?;
    let target = net.lanes[lane].at(s.min(net.lanes[lane].length())).0;
    let d = (target - v.position).truncate();
    let want = d.x.atan2(d.y).to_degrees();
    let alpha = ((want - v.heading + 540.0).rem_euclid(360.0) - 180.0) as f32;
    let speed = v.physics.velocity_kmh();
    Some((
        (alpha / 30.0).clamp(-1.0, 1.0),
        ((kmh - speed) / 10.0).clamp(0.0, 1.0),
        ((speed - kmh - 3.0) / 10.0).clamp(0.0, 1.0),
    ))
}

/// Wheel, throttle and brake that keep `v` on the lanes of `net` at `kmh` (the offscreen
/// run's autopilot). None: no street lane near.
pub(crate) fn lane_controls(net: &Network, v: &VehicleInstance, kmh: f32) -> Option<(f32, f32, f32)> {
    path_controls(net, &path_ahead(net, v.position, v.heading, AIM_AHEAD + 1.0), v, kmh)
}

/// The speed (km/h) to stop for the first light on `path` that does not let traffic go: at
/// its lane's start, 3 m short of it, closing in at 2 km/h a metre (`kmh` with none red).
/// `until_go`: the seconds until a light (controller, light) lets traffic go.
fn speed_for_lights(net: &Network, path: &[(usize, f32, f32)], kmh: f32, until_go: impl Fn(usize, usize) -> Option<f32>) -> f32 {
    path.iter()
        .skip(1)
        .find(|(l, _, _)| net.lanes[*l].traffic_light.is_some_and(|(c, li)| until_go(c, li).is_some_and(|w| w > 0.0)))
        .map_or(kmh, |(_, to_start, _)| ((to_start - 3.0 - 6.0) * 2.0).clamp(0.0, kmh))
}

/// The speed (km/h) to keep behind the nearest of `others` in the bus's path: it stops 3 m
/// short of it and closes in at 2 km/h a metre beyond that (`kmh` with nobody ahead).
pub(crate) fn speed_behind(position: DVec3, heading: f64, others: &[Obb], kmh: f32) -> f32 {
    let h = heading.to_radians();
    let (fwd, side) = (glam::DVec2::new(h.sin(), h.cos()), glam::DVec2::new(h.cos(), -h.sin()));
    let at = position.truncate();
    let gap = others
        .iter()
        .filter_map(|o| {
            let d = o.center - at;
            let (along, across) = (d.dot(fwd), d.dot(side));
            // (the bus is some 6 m from its middle to its front, the other half as long)
            let gap = along - 6.0 - o.half.y.max(o.half.x);
            (along > 0.0 && across.abs() < 2.5 + o.half.x && gap < 40.0).then_some(gap)
        })
        .fold(f64::INFINITY, f64::min);
    if gap.is_finite() {
        (((gap - 3.0) * 2.0) as f32).clamp(0.0, kmh)
    } else {
        kmh
    }
}

/// The window's autopilot: the bus put in gear D at the start, then driven by
/// [`lane_controls`] behind the traffic.
#[derive(Default)]
pub(crate) struct Autopilot {
    /// Seconds driven.
    t: f32,
    /// Gear D asked for.
    in_gear: bool,
    /// When it was last logged.
    logged: f32,
    /// Seconds it has wanted to go and stood still.
    stuck: f32,
}

impl Autopilot {
    /// The analog controls this frame (None: OMSI_AUTOPILOT is off or nothing to drive on).
    pub(crate) fn controls(
        &mut self,
        dt: f32,
        player: &mut crate::player::Player,
        traffic: Option<&crate::traffic::Traffic>,
    ) -> Option<crate::controllers::Analog> {
        let kmh = omsi_cfg::flags::OMSI_AUTOPILOT.parse::<f32>()?;
        let t = traffic?;
        self.t += dt;
        // (the foot on the brake for the first seconds: the gearbox's D wants it, and the
        // scripts set the bus up meanwhile)
        if self.t < 3.0 {
            if self.t > 1.0 && !self.in_gear {
                self.in_gear = true;
                player.action("automatic_D", true);
                player.action("automatic_D", false);
                log::info!("autopilot: gear D, driving at {kmh:.0} km/h");
            }
            return Some(crate::controllers::Analog { steering: Some(0.0), throttle: Some(0.0), brake: Some(1.0), ..Default::default() });
        }
        let v = &player.vehicle;
        let path = path_ahead(&t.net, v.position, v.heading, 70.0);
        let limit = speed_behind(v.position, v.heading, &t.boxes(v.position, 60.0), kmh).min(speed_for_lights(&t.net, &path, kmh, |c, li| t.light_until_go(c, li)));
        let (steer, throttle, brake) = path_controls(&t.net, &path, v, limit)?;
        // (stopped behind somebody: the brake held, as a driver does)
        let brake = if limit < 1.0 { brake.max(0.6) } else { brake };
        // (wanting to go and standing still: the parking brake on, or not in gear - the
        // parking brake switched over and D asked for again, every 8 s)
        let speed = v.physics.velocity_kmh();
        self.stuck = if limit > 5.0 && speed < 0.5 { self.stuck + dt } else { 0.0 };
        if self.stuck > 8.0 {
            self.stuck = 0.0;
            log::warn!("autopilot: the bus stands still - parking brake switched, gear D again");
            for name in ["parking_brake_toggle", "automatic_D"] {
                player.action(name, true);
                player.action(name, false);
            }
        }
        let v = &player.vehicle;
        if self.t - self.logged > 10.0 {
            self.logged = self.t;
            log::info!("autopilot t={:.0} s: {:.0} km/h (limit {limit:.0})", self.t, v.physics.velocity_kmh());
        }
        Some(crate::controllers::Analog { steering: Some(steer), throttle: Some(throttle), brake: Some(brake), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::traffic::LaneBuilder;

    /// A street north 0..40 m that forks at its end: on straight north (lane 1) and off to
    /// the east (lane 2); the straight one has a traffic light.
    fn fork() -> Network {
        let mut net = Network::default();
        let street = |pts: Vec<DVec3>| LaneBuilder::polyline(pts, LaneKind::Street, 3.0);
        net.lanes.push(street(vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 40.0, 0.0)]));
        net.lanes.push(street(vec![DVec3::new(0.0, 40.0, 0.0), DVec3::new(0.0, 80.0, 0.0)]));
        net.lanes.push(street(vec![DVec3::new(0.0, 40.0, 0.0), DVec3::new(10.0, 45.0, 0.0), DVec3::new(40.0, 46.0, 0.0)]));
        net.lanes[0].next = vec![2, 1];
        net.lanes[1].traffic_light = Some((0, 0));
        net
    }

    #[test]
    fn the_way_on_is_the_straightest() {
        let net = fork();
        let path = path_ahead(&net, DVec3::new(0.0, 10.0, 0.0), 0.0, 50.0);
        let lanes: Vec<usize> = path.iter().map(|p| p.0).collect();
        assert_eq!(lanes, vec![0, 1]);
        // (the bus 3 m on from where it stands is 13 m along the first lane, its end 27 m on)
        assert!((path[0].2 - 13.0).abs() < 0.5, "{path:?}");
        assert!((path[1].1 - 27.0).abs() < 0.5, "{path:?}");
    }

    #[test]
    fn it_stops_for_a_red_light_and_goes_on_green() {
        let net = fork();
        let path = path_ahead(&net, DVec3::new(0.0, 10.0, 0.0), 0.0, 50.0);
        let red = speed_for_lights(&net, &path, 40.0, |_, _| Some(12.0));
        let green = speed_for_lights(&net, &path, 40.0, |_, _| Some(0.0));
        // (27 m to the stop line: 9 m short of it to stop, 2 km/h a metre)
        assert!((red - 36.0).abs() < 1.0, "{red}");
        assert_eq!(green, 40.0);
        let near = path_ahead(&net, DVec3::new(0.0, 28.0, 0.0), 0.0, 50.0);
        assert_eq!(speed_for_lights(&net, &near, 40.0, |_, _| Some(12.0)), 0.0);
    }

    #[test]
    fn it_keeps_behind_a_car_in_its_way_only() {
        let car = |x: f64, y: f64| Obb { center: glam::DVec2::new(x, y), half: glam::DVec2::new(1.0, 2.2), heading: 0.0, z0: 0.0, z1: 1.5, velocity: glam::DVec2::ZERO, mass: 1200.0, pole: None, id: 0 };
        let at = DVec3::ZERO;
        assert_eq!(speed_behind(at, 0.0, &[], 30.0), 30.0);
        // its back 3 m from the bus's front: stopped; beside it or behind it: no matter
        assert_eq!(speed_behind(at, 0.0, &[car(0.0, 11.0)], 30.0), 0.0);
        assert_eq!(speed_behind(at, 0.0, &[car(6.0, 11.0), car(0.0, -15.0)], 30.0), 30.0);
        // 16 m between them: closing in at 26 km/h
        let v = speed_behind(at, 0.0, &[car(0.0, 24.2)], 30.0);
        assert!((v - 26.0).abs() < 0.5, "{v}");
        // driving east, the car ahead is east of it
        assert_eq!(speed_behind(at, 90.0, &[car(11.0, 0.0)], 30.0), 0.0);
    }
}

//! Approximate authored useSplines without crossing gaps or interpolating path rules.
use crate::{navigation::MAX_POINTS, paths::Point};
use anyhow::{ensure, Result};
use glam::Vec3;

pub(crate) fn smooth(points: Vec<Point>) -> Result<Vec<Point>> {
    if points.len() < 3 {
        return Ok(points);
    }
    let closed = points.first().unwrap().pos == points.last().unwrap().pos;
    let end = points.len() - 1;
    let mut out = Vec::new();
    for i in 0..end {
        let p1 = Vec3::from(points[i].pos);
        let p2 = Vec3::from(points[i + 1].pos);
        let p0 = Vec3::from(
            points[if i > 0 {
                i - 1
            } else if closed {
                end - 1
            } else {
                0
            }]
            .pos,
        );
        let p3 = Vec3::from(
            points[if i + 2 <= end {
                i + 2
            } else if closed {
                1
            } else {
                end
            }]
            .pos,
        );
        let corner = (p1 - p0)
            .normalize_or_zero()
            .dot((p2 - p1).normalize_or_zero())
            < 0.95
            || (p2 - p1)
                .normalize_or_zero()
                .dot((p3 - p2).normalize_or_zero())
                < 0.95;
        let minimum = if corner && (p2 - p1).length() > 0.25 {
            4.0
        } else {
            1.0
        };
        let steps = ((p2 - p1).length() / 5.0).ceil().clamp(minimum, 64.0) as usize;
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            let mut point = points[i].clone();
            let curve = 0.5
                * ((2. * p1)
                    + (-p0 + p2) * t
                    + (2. * p0 - 5. * p1 + 4. * p2 - p3) * t * t
                    + (-p0 + 3. * p1 - 3. * p2 + p3) * t * t * t);
            // Keep height on the authored segment; protect ramps from vertical overshoot.
            let mut pos = curve;
            pos.z = p1.z + (p2.z - p1.z) * t;
            // A short or unusually sharp segment should not bow outside its corridor.
            let straight = p1.lerp(p2, t);
            pos = straight + (pos - straight).clamp_length_max(2.0);
            ensure!(
                pos.is_finite() && pos.abs().max_element() < 1_000_000.,
                "invalid smoothed road coordinate"
            );
            point.pos = pos.to_array();
            out.push(point);
            ensure!(
                out.len() < MAX_POINTS,
                "spline smoothing exceeds path budget"
            );
        }
    }
    out.push(points[end].clone());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoothing_preserves_control_points_heights_and_authored_segment_rules() {
        let point = |pos, speed| Point {
            pos,
            speed,
            turn: 0,
            buses: true,
            cars: false,
            density: 1.,
        };
        let controls = vec![
            point([0., 0., 0.], 20.),
            point([40., 0., 2.], 40.),
            point([80., 10., 4.], 60.),
        ];
        let points = smooth(controls.clone()).unwrap();
        assert!(points.len() > controls.len());
        assert_eq!(points.first().unwrap().pos, controls[0].pos);
        assert_eq!(points.last().unwrap().pos, controls[2].pos);
        let split = points
            .iter()
            .position(|p| p.pos == controls[1].pos)
            .unwrap();
        assert!(points[..split]
            .iter()
            .all(|p| p.speed == 20. && p.buses && !p.cars && (0.0..2.0).contains(&p.pos[2])));
        assert!(points[split..points.len() - 1]
            .iter()
            .all(|p| p.speed == 40.));
    }
}

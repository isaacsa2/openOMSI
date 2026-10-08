//! The route helpers: bays, and where a bus is on a partly loaded route.

use crate::traffic::{Network, LaneBuilder, LaneKind};
use glam::DVec3;
use super::*;

#[test]
fn off_centre_bounding_boxes_align_the_physical_flank_on_either_side() {
    let mut bus = script_test_vehicle("{frame}\n{end}\n", "", "");
    let ty = std::sync::Arc::get_mut(&mut bus.ty).unwrap();
    ty.def.bounding_box = Some([2.5, 12.0, 3.0, 0.6, 0.0, 1.5]);
    let right = bay_for(4.0, ty, false, false, 0.0);
    assert!((right + 0.6 + 1.25 - 4.3).abs() < 1e-5);
    let left = bay_for(-4.0, ty, false, false, 1.0);
    assert!((left + 0.6 - 1.25 + 4.3).abs() < 1e-5);
    assert_eq!(bay_for(4.0, ty, true, false, 0.0), 0.0);
}

#[test]
fn bay_alignment_follows_the_platform_side_without_moving_rail_vehicles() {
    let bus = script_test_vehicle("{frame}\n{end}\n", "", "");
    let ty = &bus.ty;
    assert!((bay_for(4.0, ty, false, false, 0.0) - 3.05).abs() < 1e-5);
    assert!((bay_for(-4.0, ty, false, false, 1.0) + 3.05).abs() < 1e-5);
    assert!((bay_for(-4.0, ty, false, true, 0.0) + 3.05).abs() < 1e-5);
    assert!((bay_for(4.0, ty, false, true, 1.0) - 3.05).abs() < 1e-5);
    for hand in [false, true] {
        assert!((bay_for(-4.0, ty, false, hand, 2.0) + 3.05).abs() < 1e-5);
        assert!((bay_for(4.0, ty, false, hand, 2.0) - 3.05).abs() < 1e-5);
        for side in [0.0, 1.0, 2.0] {
            assert_eq!(bay_for(-4.0, ty, true, hand, side), 0.0);
            assert_eq!(bay_for(f32::NAN, ty, false, hand, side), 0.0);
        }
    }
}

#[test]
fn where_a_bus_is_on_a_partly_loaded_route() {
    let key = |id: i64| {
        Some(LaneKey {
            tile: (0, 0),
            id,
            path: 0,
        })
    };
    let legs = [0, 0, 1, 1, 1, 2];
    let steps: Vec<Step> = legs
        .iter()
        .enumerate()
        .map(|(i, &leg)| Step {
            key: key(i as i64),
            leg,
            length: 0.0,
        })
        .collect();
    let slots = [
        Slot::Lane(10),
        Slot::Lane(11),
        Slot::Lane(12),
        Slot::Waiting,
        Slot::Lane(14),
        Slot::Absent,
    ];
    let est = [100.0, 100.0, 50.0, 70.0, 50.0, 0.0];
    // a layover bus stands at the start
    assert_eq!(step_at(&steps, &slots, &est, 0, 0.0), Some((0, 0.0)));
    // 17 m into leg 1: on its first lane, whose part of the route ends at the gap
    let (at, off) = step_at(&steps, &slots, &est, 1, 0.1).unwrap();
    assert!(at == 2 && (off - 17.0).abs() < 1e-9, "{at} {off}");
    assert_eq!(section_around(&slots, 2), (0, 3));
    // half way: on the step still to come - the bus has to wait
    assert_eq!(step_at(&steps, &slots, &est, 1, 0.5), Some((3, 35.0)));
    // near the end of the leg: after the gap
    let (at, off) = step_at(&steps, &slots, &est, 1, 0.9).unwrap();
    assert_eq!(at, 4);
    assert!((off - 33.0).abs() < 1e-9);
    assert_eq!(section_around(&slots, 4), (4, 6));
    // a leg of absent steps only, and a leg without steps: past the end
    assert_eq!(step_at(&steps, &slots, &est, 2, 0.5), None);
    assert_eq!(step_at(&steps, &slots, &est, 3, 0.5), None);
    // a leg without a station link: at the start of the next leg
    let steps2: Vec<Step> = [0, 2, 2]
        .iter()
        .enumerate()
        .map(|(i, &leg)| Step {
            key: key(i as i64),
            leg,
            length: 0.0,
        })
        .collect();
    let slots2 = [Slot::Lane(1), Slot::Absent, Slot::Lane(3)];
    assert_eq!(
        step_at(&steps2, &slots2, &[10.0, 0.0, 10.0], 1, 0.5),
        Some((2, 0.0))
    );
    assert_eq!(section_around(&slots2, 2), (0, 3));
}

#[test]
fn bays() {
    // Geometric stops retain their box offset until the vehicle is known.
    for lat in [0.0, 2.0, -4.0] {
        assert_eq!(bay_offset(lat, StopRoute::Nearest), lat);
        assert!(bay_offset(lat, StopRoute::Track(0)).is_nan());
    }
}

#[test]
fn authored_track_stop_keeps_ai_on_the_route_path() {
    let bus = script_test_vehicle("{frame}\n{end}\n", "", "");
    let ty = &bus.ty;
    let left_platform = -4.0;

    // An exact timetable track entry already tells the AI where to drive laterally.
    // The stop object only supplies the longitudinal stop position in this case.
    assert_eq!(
        bay_for(
            bay_offset(left_platform, StopRoute::Track(0)),
            ty,
            false,
            false,
            1.0,
        ),
        0.0
    );

    // Untyped/fallback stops retain the existing bay alignment behaviour.
    assert_ne!(
        bay_for(
            bay_offset(left_platform, StopRoute::Nearest),
            ty,
            false,
            false,
            1.0,
        ),
        0.0
    );
}

#[test]
fn authored_bays_ignore_box_side_traffic_hand_and_rail_kind() {
    let bus = script_test_vehicle("{frame}\n{end}\n", "", "");
    for lat in [-4.0, 0.0, 4.0] {
        for side in [0.0, 1.0, 2.0] {
            for left_hand in [false, true] {
                for rail in [false, true] {
                    assert_eq!(
                        bay_for(bay_offset(lat, StopRoute::Track(2)), &bus.ty, rail, left_hand, side),
                        0.0,
                        "authored visit: lat={lat}, side={side}, left_hand={left_hand}, rail={rail}",
                    );
                }
            }
        }
    }
}

#[test]
fn authored_platform_boxes_on_curves_keep_the_repeated_visit_and_path() {
    let bus = script_test_vehicle("{frame}\n{end}\n", "", "");
    for radius in [-40.0, 40.0] {
        let net = Network {
            lanes: vec![LaneBuilder::arc(DVec3::ZERO, 0.0, 60.0, radius, 0.0, LaneKind::Street, 3.0)],
            ..Default::default()
        };
        let (point, heading) = net.lanes[0].at(30.0);
        let h = (heading as f64).to_radians();
        let right = DVec3::new(h.cos(), -h.sin(), 0.0);
        for offset in [-4.0, 4.0] {
            for side in [0.0, 1.0, 2.0] {
                let visit = StopRoute::Track(1);
                let (ri, s, lat) = project_stop(&net, &[0, 0], point + right * offset, Some(25.0), 0, side, visit).unwrap();
                assert_eq!(ri, 1, "the authored repeated visit is retained");
                assert!((s - 30.0).abs() < 0.5, "projection keeps the position along the curve: {s}");
                assert!(lat.abs() > 3.5, "the platform box lies off the path");
                let mut stops = [(ri, s, bay_offset(lat, visit), 0.0, 42, side)];
                place_stops(&net, &[0, 0], 0, &mut stops, &bus.ty, false);
                assert_eq!(stops[0].2, 0.0, "no bay target on either curve/platform side");
                assert!(stops[0].1.is_finite());
            }
        }
    }
}

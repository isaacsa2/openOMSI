use super::*;

fn point(x: f32, y: f32, z: f32) -> Point {
    Point {
        pos: [x, y, z],
        speed: 50.0,
        turn: 0,
        buses: false,
        cars: true,
        density: 1.0,
    }
}

fn chain(points: &[(f32, f32)]) -> DrivePath {
    DrivePath {
        points: points.iter().map(|&(x, y)| point(x, y, 0.0)).collect(),
        pedestrian: false,
    }
}

type Lane = (Vec3, Vec3);

fn lanes(paths: &[DrivePath]) -> Vec<Lane> {
    paths
        .iter()
        .filter(|p| !p.pedestrian)
        .flat_map(|p| {
            p.points
                .windows(2)
                .filter_map(|w| {
                    let (a, b) = (Vec3::from(w[0].pos), Vec3::from(w[1].pos));
                    ((b - a).truncate().length() >= 0.01).then_some((a, b))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn heading(l: &Lane) -> Vec2 {
    (l.1 - l.0).truncate().normalize()
}

/// Does the native network join lane `i` to lane `j`? (The rule of `omsi-sim`'s lane links.)
fn joins(l: &[Lane], i: usize, j: usize) -> bool {
    i != j
        && (l[j].0 - l[i].1).truncate().length() <= 1.5
        && (l[j].0.z - l[i].1.z).abs() < 3.0
        && turn(heading(&l[i]), heading(&l[j])) < 40.0
}

fn successors(l: &[Lane], i: usize) -> usize {
    (0..l.len()).filter(|&j| joins(l, i, j)).count()
}

/// Lanes the network gives no next lane (the last lane of a chain is always one).
fn dead_lanes(l: &[Lane]) -> usize {
    (0..l.len()).filter(|&i| successors(l, i) == 0).count()
}

/// The sharpest heading change between two consecutive lanes of one chain.
fn sharpest(path: &DrivePath) -> f32 {
    let l = lanes(std::slice::from_ref(path));
    (1..l.len())
        .map(|i| turn(heading(&l[i - 1]), heading(&l[i])))
        .fold(0.0, f32::max)
}

fn repaired(mut paths: Vec<DrivePath>) -> (Vec<DrivePath>, Report) {
    let mut report = Report::default();
    let extra = connect(&mut paths, &mut report);
    paths.extend(extra);
    (paths, report)
}

fn xy(p: &Point) -> (f32, f32) {
    (p.pos[0], p.pos[1])
}

#[test]
fn a_right_angle_inside_a_chain_is_rounded_so_every_lane_joins_the_next() {
    let before = vec![chain(&[(0.0, 0.0), (0.0, 20.0), (20.0, 20.0)])];
    assert!(sharpest(&before[0]) > 85.0);
    assert_eq!(
        dead_lanes(&lanes(&before)),
        2,
        "a car reaching the corner has no next lane"
    );
    let (after, report) = repaired(before.clone());
    let l = lanes(&after);
    assert!(sharpest(&after[0]) < 38.0, "{}", sharpest(&after[0]));
    assert_eq!(
        dead_lanes(&l),
        1,
        "only the last lane of the chain is an end"
    );
    assert_eq!(report.road_corners_rounded, 1);
    // every lane but the last leads on, so a car goes round the corner and out
    assert!((0..l.len() - 1).all(|i| successors(&l, i) == 1), "{:?}", l);
    // the ends of the chain did not move
    let p = &after[0].points;
    assert_eq!(xy(&p[0]), (0.0, 0.0));
    assert_eq!(xy(p.last().unwrap()), (20.0, 20.0));
    // the curve starts and ends on the two lanes
    assert!(p.iter().any(|q| xy(q) == (0.0, 12.0)) && p.iter().any(|q| xy(q) == (8.0, 20.0)));
}

#[test]
fn gentle_corners_hairpins_and_straight_chains_are_left_as_drawn() {
    // 30 degrees is under the limit for rounding, 150 degrees is a turn-back the author drew
    let gentle = chain(&[(0.0, 0.0), (0.0, 20.0), (10.0, 37.3)]);
    let hairpin = chain(&[(50.0, 0.0), (50.0, 20.0), (48.0, 0.5)]);
    let straight = chain(&[(100.0, 0.0), (100.0, 10.0), (100.0, 30.0)]);
    let (after, report) = repaired(vec![gentle.clone(), hairpin.clone(), straight.clone()]);
    for (a, b) in after.iter().zip([&gentle, &hairpin, &straight]) {
        assert_eq!(
            a.points.iter().map(xy).collect::<Vec<_>>(),
            b.points.iter().map(xy).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        (
            report.road_corners_rounded,
            report.road_joints_connected,
            report.road_gaps_bridged
        ),
        (0, 0, 0)
    );
}

#[test]
fn a_curve_never_turns_more_than_the_native_limit_between_two_lanes() {
    for degrees in [37.0f32, 45.0, 60.0, 90.0, 110.0] {
        let r = degrees.to_radians();
        let corner = chain(&[
            (0.0, 0.0),
            (0.0, 30.0),
            (30.0 * r.sin(), 30.0 + 30.0 * r.cos()),
        ]);
        let (after, report) = repaired(vec![corner]);
        let l = lanes(&after);
        assert_eq!(report.road_corners_rounded, 1, "{degrees}");
        let worst = (0..l.len() - 1)
            .map(|i| turn(heading(&l[i]), heading(&l[i + 1])))
            .fold(0.0, f32::max);
        assert!(
            worst < 38.0,
            "{degrees} degrees: a lane turns {worst} from the next"
        );
    }
}

#[test]
fn short_lanes_give_up_only_a_share_of_their_length_to_the_curve() {
    // lanes of 4 m: 8 m of radius would cut past their ends
    let (after, _) = repaired(vec![chain(&[(0.0, 0.0), (0.0, 4.0), (4.0, 4.0)])]);
    let p = &after[0].points;
    let first_cut = p.iter().find(|q| xy(q) != (0.0, 0.0)).unwrap();
    assert!(
        first_cut.pos[1] >= 4.0 * (1.0 - SHARE) - 1e-4,
        "{:?}",
        xy(first_cut)
    );
    let last_cut = p.iter().rev().find(|q| xy(q) != (4.0, 4.0)).unwrap();
    assert!(last_cut.pos[0] <= 4.0 * SHARE + 1e-4, "{:?}", xy(last_cut));
    assert!(sharpest(&after[0]) < 38.0, "{}", sharpest(&after[0]));
    assert_eq!(dead_lanes(&lanes(&after)), 1);
}

#[test]
fn rounding_keeps_the_corners_rules_and_the_height() {
    let mut corner = chain(&[(0.0, 0.0), (0.0, 20.0), (20.0, 20.0)]);
    corner.points[0].pos[2] = 0.0;
    corner.points[1] = Point {
        speed: 30.0,
        turn: 2,
        density: 2.0,
        ..point(0.0, 20.0, 4.0)
    };
    corner.points[2].pos[2] = 8.0;
    let (after, _) = repaired(vec![corner]);
    let p = &after[0].points;
    let start = p.iter().position(|q| xy(q) == (0.0, 12.0)).unwrap();
    let end = p.iter().position(|q| xy(q) == (8.0, 20.0)).unwrap();
    for q in &p[start..=end] {
        assert_eq!(
            (q.speed, q.turn),
            (30.0, 2),
            "the curve is the corner: blinker and speed"
        );
    }
    assert!(
        p[start..end].iter().all(|q| q.density == 0.0),
        "no car appears inside the curve"
    );
    assert_eq!(
        p[end].density, 2.0,
        "the corner's spawner is where its lane begins"
    );
    assert!(
        p.windows(2).all(|w| w[0].pos[2] <= w[1].pos[2] + 1e-4),
        "height never goes back"
    );
}

#[test]
fn two_chains_meeting_at_a_right_angle_are_joined_by_a_curve() {
    let before = vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(0.5, 20.5), (20.0, 20.5)]),
    ];
    assert_eq!(
        dead_lanes(&lanes(&before)),
        2,
        "the first road ends at the corner"
    );
    let (after, report) = repaired(before);
    let l = lanes(&after);
    assert_eq!(
        dead_lanes(&l),
        1,
        "only the very last lane is an end: {l:?}"
    );
    assert_eq!(report.road_joints_connected, 1);
    assert_eq!(after.len(), 3, "the two chains and the curve between them");
    // the first lane of the first chain reaches the curve, the curve reaches the second chain
    let first = l
        .iter()
        .position(|x| x.0 == Vec3::new(0.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(successors(&l, first), 1);
    let on = (0..l.len()).filter(|&i| successors(&l, i) == 1).count();
    assert_eq!(on, l.len() - 1, "only the very last lane is an end");
}

#[test]
fn a_junction_that_already_joins_is_never_cut() {
    // A ends where B1 goes straight on and B2 turns right: the right turn is added, the
    // straight road keeps its link and A's end does not move
    let before = vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(0.0, 20.0), (0.0, 40.0)]),
        chain(&[(0.0, 20.3), (20.0, 20.3)]),
    ];
    let a_end = before[0].points[1].pos;
    let (after, report) = repaired(before);
    assert_eq!(
        after[0].points.last().unwrap().pos,
        a_end,
        "an end with a link keeps its place"
    );
    let l = lanes(&after);
    assert_eq!(
        dead_lanes(&l),
        2,
        "the two roads that go on end where they end"
    );
    let a = l
        .iter()
        .position(|x| x.0.truncate() == Vec2::ZERO && x.1.y == 20.0)
        .unwrap();
    let straight = l
        .iter()
        .position(|x| x.0 == Vec3::new(0.0, 20.0, 0.0) && x.1 == Vec3::new(0.0, 40.0, 0.0))
        .unwrap();
    assert!(joins(&l, a, straight), "the straight road keeps its link");
    assert!(successors(&l, a) >= 2, "and the right turn is added");
    assert_eq!(report.road_joints_connected, 1);
}

#[test]
fn a_corner_where_another_chain_ends_is_left_alone() {
    // C ends half a metre from the corner of X: pulling X's lanes away from it would cut C off
    let x = chain(&[(0.0, 0.0), (0.0, 20.0), (20.0, 20.0)]);
    let c = chain(&[(-10.0, 20.0), (-0.5, 20.0)]);
    let (after, report) = repaired(vec![x.clone(), c]);
    assert_eq!(
        after[0].points.iter().map(xy).collect::<Vec<_>>(),
        x.points.iter().map(xy).collect::<Vec<_>>()
    );
    assert_eq!(report.road_corners_rounded, 0);
}

#[test]
fn a_hole_is_closed_only_between_a_dead_end_and_a_compatible_start() {
    let straight_on = vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(0.0, 24.0), (0.0, 44.0)]),
    ];
    let (after, report) = repaired(straight_on);
    assert_eq!(report.road_gaps_bridged, 1);
    assert_eq!(after.len(), 3);
    let l = lanes(&after);
    assert_eq!(successors(&l, 0), 1, "the first lane now leads on");
    // a start to the side, one too far away, one behind: nothing to close
    for second in [
        [(4.0, 20.0), (24.0, 20.0)],
        [(0.0, 30.0), (0.0, 50.0)],
        [(0.0, 14.0), (0.0, -6.0)],
    ] {
        let (after, report) = repaired(vec![chain(&[(0.0, 0.0), (0.0, 20.0)]), chain(&second)]);
        assert_eq!(
            (report.road_gaps_bridged, after.len()),
            (0, 2),
            "{second:?}"
        );
    }
}

#[test]
fn walkers_loops_and_open_ends_are_untouched() {
    let walker = DrivePath {
        pedestrian: true,
        ..chain(&[(0.0, 0.0), (0.0, 20.0), (20.0, 20.0)])
    };
    let looped = chain(&[(100.0, 0.0), (100.0, 20.0), (120.0, 20.0), (100.0, 0.0)]);
    let open = chain(&[(200.0, 0.0), (200.0, 50.0)]);
    let (after, report) = repaired(vec![walker.clone(), looped.clone(), open.clone()]);
    assert_eq!(after.len(), 3);
    for (a, b) in after.iter().zip([&walker, &looped, &open]) {
        assert_eq!(
            a.points.iter().map(xy).collect::<Vec<_>>(),
            b.points.iter().map(xy).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        (
            report.road_corners_rounded,
            report.road_joints_connected,
            report.road_gaps_bridged
        ),
        (0, 0, 0)
    );
}

#[test]
fn a_hairpin_between_two_chains_stays_a_turn_back() {
    let before = vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(0.5, 20.2), (0.5, 0.0)]),
    ];
    let (after, report) = repaired(before);
    assert_eq!((after.len(), report.road_joints_connected), (2, 0));
}

#[test]
fn the_ends_that_remain_are_counted_and_the_first_are_named() {
    // two roads that do not meet: both of their ends are open, the one with a neighbour first
    let paths = vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(8.0, 20.0), (8.0, 40.0)]),
    ];
    let (_, report) = repaired(paths);
    assert_eq!(report.road_ends_open, 2);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.contains("no continuation"))
        .unwrap();
    assert!(
        warning.starts_with("2 vehicle path ends") && warning.contains("(0, 20)"),
        "{warning}"
    );
    // a joined road leaves only its own last end
    let (_, report) = repaired(vec![
        chain(&[(0.0, 0.0), (0.0, 20.0)]),
        chain(&[(0.0, 20.0), (0.0, 40.0)]),
    ]);
    assert_eq!(report.road_ends_open, 1);
}

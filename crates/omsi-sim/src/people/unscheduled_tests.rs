use super::*;
use crate::collision::{CollisionWorld, Obb};
use crate::world_data::ObjectPlaces;

struct StopWorld {
    unscheduled: bool,
    objects: ObjectPlaces,
    stops: Vec<(i64, DVec3, f64, String)>,
    waits: Vec<(i64, DVec3, f64, f32)>,
    collision: Arc<CollisionWorld>,
    parked: Arc<Vec<Obb>>,
}
impl StopWorld {
    fn new(unscheduled: bool) -> Self {
        let mut objects = ObjectPlaces::default();
        objects.add_index([(2, (DVec3::X * 3., [0.; 3])), (3, (DVec3::X * 4., [0.; 3]))], Vec::new());
        Self {
            unscheduled,
            objects,
            stops: vec![(1, DVec3::ZERO, 0., "Fixture stop".into())],
            waits: vec![(2, DVec3::X * 3., 0., 0.), (3, DVec3::X * 4., 0., 0.)],
            collision: Arc::new(CollisionWorld::default()),
            parked: Arc::new(vec![]),
        }
    }
}
impl World for StopWorld {
    fn root(&self) -> &Path {
        Path::new(".")
    }
    fn map_dir(&self) -> &Path {
        Path::new(".")
    }
    fn unscheduled_passengers(&self) -> bool {
        self.unscheduled
    }
    fn walk_height(&self, _: f64, _: f64) -> Option<f64> {
        Some(0.)
    }
    fn walk_height_near(&self, _: f64, _: f64, _: f64) -> Option<f64> {
        Some(0.)
    }
    fn has_ground(&self, _: f64, _: f64) -> bool {
        true
    }
    fn stop_exit_weight(&self, _: i64) -> f32 {
        0.5
    }
    fn stop_enter(&self, _: i64) -> (f32, f32) {
        (2., 2.)
    }
    fn stop_side(&self, _: i64) -> f32 {
        0.
    }
    fn stop_length(&self, _: i64) -> f32 {
        30.
    }
    fn bus_stops(&self) -> &[(i64, DVec3, f64, String)] {
        &self.stops
    }
    fn waiting_places(&self) -> &[(i64, DVec3, f64, f32)] {
        &self.waits
    }
    fn object_positions(&self) -> &ObjectPlaces {
        &self.objects
    }
    fn collision(&self) -> &Arc<CollisionWorld> {
        &self.collision
    }
    fn parked_boxes(&self) -> &Arc<Vec<Obb>> {
        &self.parked
    }
    fn tiles_generation(&self) -> u64 {
        0
    }
}

#[test]
fn imported_stop_demand_spawns_without_trips_but_respects_a_real_timetable() {
    let dir = std::env::temp_dir().join(format!("unscheduled-pax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    for (unscheduled, has_trips, want) in [(false, false, 0), (true, false, 2), (true, true, 0)] {
        let world = StopWorld::new(unscheduled);
        let mut sim = PeopleSim::new(&dir, 20);
        sim.types = vec![ty.clone()];
        // A present-but-empty Schedule used to suppress every imported stop.
        let targets = if has_trips {
            [(99, vec![("Elsewhere".into(), HashSet::new())])]
                .into_iter()
                .collect()
        } else {
            HashMap::new()
        };
        sim.stop_targets = Some(targets);
        sim.due_dests = Some(HashMap::new());
        sim.populate(&world, DVec3::ZERO);
        assert_eq!(sim.stops[&1].spots.len(), 2);
        sim.stops_tick(0.02, &world);
        assert_eq!(sim.stops[&1].want, want);
        assert_eq!(sim.people.len(), want);
        if want > 0 {
            assert!(sim
                .people
                .iter()
                .all(|p| matches!(&p.state, State::Pax(x) if x.stop == Some(1))));
            sim.stops
                .get_mut(&1)
                .unwrap()
                .buses
                .push((BusId::Player, true));
            assert!(sim.bus_for(0, 1, &[], &HashMap::new()).is_some());
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

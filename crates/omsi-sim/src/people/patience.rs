//! Patience at the stop, an option (`PeopleSim::patience`; not Omsi.exe's): somebody who has
//! waited longer than they would and sees no bus of theirs coming leaves the stop and walks
//! off, and who steps into the player's bus after a long wait, or into a crowded one, is
//! annoyed - the trip's satisfaction in its summary.

use super::model::{BusId, PeopleSim, State};
use super::pax::Task;

/// The longest anybody waits (s): 10 to 25 minutes, the same for a person every time it is
/// asked (drawn from the id, not the random sequence the rest of the people draw from).
pub fn patience_of(id: u32) -> f32 {
    600.0 + 900.0 * unit(id)
}

/// Waited past this share of their patience, they step in annoyed.
pub const ANNOYED_AT: f32 = 0.6;

/// From this share of the places taken, a bus is crowded.
pub const CROWDED_AT: f32 = 0.85;

/// `id` spread over 0..1 (a 32-bit mix: neighbouring ids land far apart).
fn unit(id: u32) -> f32 {
    let mut x = id.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Is a bus with these places taken (`true`) crowded?
pub fn crowded(taken: &[bool]) -> bool {
    !taken.is_empty() && taken.iter().filter(|t| **t).count() as f32 >= CROWDED_AT * taken.len() as f32
}

impl PeopleSim {
    /// Has waiting person `i` waited past their patience (with the option on; never one of
    /// the host's people drawn here, nor somebody late for a bus)?
    pub(super) fn out_of_patience(&self, i: usize) -> bool {
        if !self.patience || self.people[i].remote {
            return false;
        }
        match &self.people[i].state {
            State::Pax(x) => x.task == Task::WaitingForBus && x.inside.is_none() && x.late.is_none() && x.waited > patience_of(self.people[i].id),
            _ => false,
        }
    }

    /// Waiting person `i` gives up: their place at stop `stop` is free again and they walk
    /// off along the pavement, as somebody knocked down there does.
    pub(super) fn give_up(&mut self, i: usize, stop: i64, remove: &mut Vec<usize>) {
        let Some((at, heading)) = self.pax(i).map(|x| (x.pos, x.yaw.to_degrees())) else { return };
        if super::debug_pax() {
            log::info!("t={:.1} pax {} gives up waiting at stop {stop}", self.time, self.people[i].label());
        }
        self.release(i);
        self.walk_street_plain(i, at, heading, Some(stop), remove);
        self.gave_up += 1;
    }

    /// Is person `i`, stepping into bus `bus`, annoyed: a long wait, or a crowded bus?
    pub(super) fn annoyed_stepping_in(&self, i: usize, bus: BusId) -> bool {
        if !self.patience {
            return false;
        }
        let waited = self.pax(i).map_or(0.0, |x| x.waited);
        waited > ANNOYED_AT * patience_of(self.people[i].id) || self.seats.get(&bus).is_some_and(|t| crowded(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patience_is_ten_to_twenty_five_minutes_and_fixed_per_person() {
        let all: Vec<f32> = (0..10_000).map(patience_of).collect();
        assert!(all.iter().all(|p| (600.0..1500.0).contains(p)));
        assert_eq!(patience_of(42), patience_of(42));
        // (spread over the range, not bunched: neighbours differ)
        let mean = all.iter().sum::<f32>() / all.len() as f32;
        assert!((mean - 1050.0).abs() < 25.0, "{mean}");
        assert_ne!(patience_of(1), patience_of(2));
    }

    /// Out of patience with no bus of theirs about: they leave the stop and walk off, and
    /// are counted; with the option off, or still within their patience, they wait on.
    #[test]
    fn somebody_out_of_patience_leaves_the_stop() {
        use hashbrown::HashMap;
        let world = crate::world_data::WorldData::default();
        let run = |patience: bool, waited: f32| {
            let (mut h, _bus) = crate::people::pax_stops::brt_tests::setup(&format!("patience-{patience}-{waited}"), None);
            h.patience = patience;
            let p = h.pax_mut(0).unwrap();
            (p.task, p.bus, p.waited) = (Task::WaitingForBus, None, waited);
            h.pax_task(0, 0.5, &world, &[], &HashMap::new(), &HashMap::new(), None, &mut false, &mut Vec::new());
            h
        };
        let limit = patience_of(1);
        let h = run(true, limit + 1.0);
        assert!(h.pax(0).is_none(), "walked off: a pedestrian now");
        assert_eq!(h.gave_up, 1);
        let h = run(true, limit - 10.0);
        assert_eq!(h.pax(0).map(|p| p.task), Some(Task::WaitingForBus));
        assert_eq!(h.pax(0).unwrap().waited, limit - 9.5, "the wait goes on counting");
        let h = run(false, limit + 1.0);
        assert_eq!(h.pax(0).map(|p| p.task), Some(Task::WaitingForBus), "the option off: as Omsi.exe");
        assert_eq!(h.gave_up, 0);
    }

    /// Stepping in after a long wait, or into a crowded bus, annoys; only with the option on.
    #[test]
    fn a_long_wait_or_a_crowded_bus_annoys() {
        let (mut h, _bus) = crate::people::pax_stops::brt_tests::setup("annoyed", None);
        h.patience = true;
        let limit = patience_of(1);
        h.pax_mut(0).unwrap().waited = 0.5 * limit;
        assert!(!h.annoyed_stepping_in(0, BusId::Player));
        h.pax_mut(0).unwrap().waited = 0.7 * limit;
        assert!(h.annoyed_stepping_in(0, BusId::Player));
        h.pax_mut(0).unwrap().waited = 0.0;
        h.seats.insert(BusId::Player, vec![true; 10]);
        assert!(h.annoyed_stepping_in(0, BusId::Player), "crowded");
        h.patience = false;
        assert!(!h.annoyed_stepping_in(0, BusId::Player));
    }

    #[test]
    fn a_bus_is_crowded_from_85_per_cent_of_its_places() {
        assert!(!crowded(&[]));
        assert!(!crowded(&[true, true, true, true, false, false, false, false, false, false]));
        let mut taken = vec![true; 17];
        taken.extend([false; 3]);
        assert!(crowded(&taken));
        taken[0] = false;
        assert!(!crowded(&taken));
    }
}

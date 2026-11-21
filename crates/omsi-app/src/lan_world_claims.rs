//! Client-side reliable boarding completion and rollback for headless GRANT.
//! Keep all transport retries outside the already large graphical lan_world module.

use super::*;

impl Mirror {
    /// A granted person is the client's only until boarding is confirmed.
    /// If its bus passes the platform before boarding, the client deletes
    /// its own copy and retries PAXRETURN until the native host acknowledges.
    pub(super) fn settle_passenger_claims(
        &mut self,
        lan: &mut LanSession,
        h: &mut Humans,
        view: &mut SimView,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
    ) {
        // Host acknowledges both the physical-boarding notification and a
        // cancelled claim. Never stop retrying a UDP return before PAXACK.
        for id in lan.take_pax_acks() {
            self.return_pending.remove(&id);
            self.boarded_pending.remove(&id);
        }
        // A GRANT is only a provisional transfer. Keep its original identity
        // until the person physically crosses the player's bus boundary.
        // Passing the point without boarding returns it to the next driver.
        let current_bus = h
            .last_buses
            .iter()
            .find(|bus| bus.id == omsi_sim::people::BusId::Player);
        let mut finished = Vec::new();
        let mut cancelled = Vec::new();
        for (&id, &(stop, received)) in &self.granted_waiting {
            let Some(person) = h.people.iter().find(|p| p.id == id) else {
                // Locally retired/disappeared people must not be recreated
                // without proof that they stayed outside the bus.
                finished.push(id);
                continue;
            };
            if matches!(
                person.place,
                omsi_sim::people::Place::Bus(omsi_sim::people::BusId::Player, _)
            ) {
                self.boarded_pending.insert(id, None);
                finished.push(id);
                continue;
            }
            if received.elapsed() < std::time::Duration::from_secs(2) {
                continue;
            }
            let stop_position = h.stops.get(&stop).map(|s| s.pos);
            let moved_away = match (current_bus, stop_position) {
                (Some(bus), Some(point)) => {
                    let distance = (bus.pos - point).truncate().length();
                    distance > 75.0 || (distance > 25.0 && bus.speed.abs() > 2.0)
                }
                (None, _) => true,
                _ => false,
            };
            if moved_away {
                cancelled.push(id);
                finished.push(id);
            }
        }
        for id in finished {
            self.granted_waiting.remove(&id);
        }
        if !cancelled.is_empty() {
            for id in &cancelled {
                if let Some(index) = h.people.iter().position(|p| p.id == *id) {
                    // Only the not-yet-boarded people can be removed here.
                    if matches!(h.people[index].place, omsi_sim::people::Place::Ground) {
                        h.release(index);
                        let person = h.people.swap_remove(index);
                        h.retire(&person);
                        self.return_pending.insert(*id, None);
                    }
                }
            }
            crate::view_sync::apply_people(h, &mut view.people, world, renderer, scene);
        }
        // Retry both positive boarding acknowledgements and cancellations
        // once per second. The server restores each original person at most
        // once and sends an idempotent PAXACK to stop the retries.
        let now = Instant::now();
        let due = |last: &Option<Instant>| {
            last.is_none_or(|at| now.duration_since(at) >= std::time::Duration::from_secs(1))
        };
        let returning: Vec<u32> = m
            .return_pending
            .iter()
            .filter(|(_, at)| due(at))
            .map(|(id, _)| *id)
            .collect();
        if !returning.is_empty() {
            lan.return_unboarded(&returning);
            for id in returning {
                self.return_pending.insert(id, Some(now));
            }
        }
        let boarded: Vec<u32> = m
            .boarded_pending
            .iter()
            .filter(|(_, at)| due(at))
            .map(|(id, _)| *id)
            .collect();
        if !boarded.is_empty() {
            lan.confirm_boarded(&boarded);
            for id in boarded {
                self.boarded_pending.insert(id, Some(now));
            }
        }
    }
}

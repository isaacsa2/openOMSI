//! Client-side reliable boarding completion and rollback for headless GRANT.

use super::*;

impl Mirror {
    /// A granted person is the client's only until boarding is confirmed.
    pub(super) fn settle_passenger_claims(
        &mut self,
        lan: &mut LanSession,
        h: &mut Humans,
        view: &mut SimView,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
    ) {
        for id in lan.take_pax_acks() {
            self.return_pending.remove(&id);
            self.boarded_pending.remove(&id);
        }
        let current_bus = h
            .last_buses
            .iter()
            .find(|bus| bus.id == omsi_sim::people::BusId::Player);
        let mut finished = Vec::new();
        let mut cancelled = Vec::new();
        for (&id, &(stop, received)) in &self.granted_waiting {
            let Some(person) = h.people.iter().find(|p| p.id == id) else {
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
        let now = Instant::now();
        resend_until_ack(lan, &mut self.return_pending, now, false);
        resend_until_ack(lan, &mut self.boarded_pending, now, true);
    }
}

fn resend_until_ack(
    lan: &mut LanSession,
    pending: &mut HashMap<u32, Option<Instant>>,
    now: Instant,
    boarded: bool,
) {
    let due: Vec<u32> = pending
        .iter()
        .filter(|(_, last)| {
            last.is_none_or(|at| now.duration_since(at) >= std::time::Duration::from_secs(1))
        })
        .map(|(id, _)| *id)
        .collect();
    if due.is_empty() {
        return;
    }
    if boarded {
        lan.confirm_boarded(&due);
    } else {
        lan.return_unboarded(&due);
    }
    for id in due {
        pending.insert(id, Some(now));
    }
}

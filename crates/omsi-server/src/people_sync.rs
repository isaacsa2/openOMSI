//! Authoritative pedestrian snapshots for the renderer-free host.
//!
//! Uses the existing WORLD and DESC protocols. Physics and population are
//! separate: experimental free-roam currently creates no pedestrians, but
//! this publisher is ready to handle future CPU-simulated pedestrians.

use glam::DVec3;
use omsi_net::traffic_table::TrafficTable;
use omsi_net::world::{self, Activity, Desc, PersonPlace, PersonState, WorldFrame};
use omsi_net::{LanSession, Role};
use omsi_sim::people::{LanPerson, PeopleSim};
use std::collections::{HashMap, HashSet};

const PERSON_RADIUS: f64 = 260.0;
const MAX_PEOPLE_PER_CLIENT: usize = 96;
const MAX_WANTS_PER_CLIENT: usize = 64;
const MAX_GONE_PER_FRAME: usize = 63;

#[derive(Default)]
struct ClientPeople {
    sent: HashMap<u32, (PersonState, f32)>,
    described: HashSet<u32>,
}

impl ClientPeople {
    fn frame(&mut self, visible: &[PersonState], dt: f32) -> WorldFrame {
        let mut frame = WorldFrame::default();
        let seen: HashSet<u32> = visible.iter().map(|p| p.id).collect();
        for state in visible {
            let due = self
                .sent
                .get(&state.id)
                .is_none_or(|(old, age)| old != state || *age >= world::STANDING_EVERY);
            if due {
                self.sent.insert(state.id, (*state, 0.0));
                frame.people.push(*state);
            } else if let Some((_, age)) = self.sent.get_mut(&state.id) {
                *age += dt;
            }
        }
        let gone: Vec<u32> = self
            .sent
            .keys()
            .filter(|id| !seen.contains(id))
            .take(MAX_GONE_PER_FRAME)
            .copied()
            .collect();
        for id in gone {
            self.sent.remove(&id);
            self.described.remove(&id);
            frame.gone.push((true, id));
        }
        frame
    }
}

#[derive(Default)]
pub struct HeadlessPeopleSync {
    elapsed: f32,
    clients: HashMap<u32, ClientPeople>,
    pub sent_bytes: u64,
}

fn state_of(person: &LanPerson, traffic: &TrafficTable) -> Option<PersonState> {
    if person.id > world::MAX_ID
        || !person.pos.is_finite()
        || !person.heading.is_finite()
        || !person.speed.is_finite()
        || person.pos.x.abs() > 1.0e8
        || person.pos.y.abs() > 1.0e8
        || person.pos.z.abs() > 1.0e8
    {
        return None;
    }
    let place = match person.aboard {
        Some((bus, local, heading, seat)) => {
            // A client cannot materialize an AI rider until the bus has an
            // acknowledged traffic row. Never invent one or use raw sim IDs.
            let bus = traffic.id_of(bus)?.raw();
            if !local.is_finite() || !heading.is_finite() {
                return None;
            }
            PersonPlace::Aboard {
                bus,
                x: local.x,
                y: local.y,
                z: local.z,
                heading: heading as f32,
                seat: seat.map(|index| index.min(254) as u8),
            }
        }
        None => PersonPlace::Foot {
            x: person.pos.x,
            y: person.pos.y,
            z: person.pos.z,
            heading: person.heading as f32,
            speed: person.speed.max(0.0).min(20.0) as f32,
            waiting: person
                .waiting
                .filter(|(stop, _)| (0..=u32::MAX as i64).contains(stop))
                .map(|(stop, index)| (stop, index.min(255) as u8)),
        },
    };
    Some(PersonState {
        id: person.id,
        activity: omsi_lan::mirror::net_activity(person.activity),
        place,
    })
}

impl HeadlessPeopleSync {
    /// Send descriptions and changed positions only to players within the
    /// original host's interest radius. Late joiners receive full snapshots;
    /// despawns and out-of-range people receive GONE notifications.
    pub fn tick(
        &mut self,
        lan: &mut LanSession,
        people: &PeopleSim,
        table: &TrafficTable,
        dt: f32,
    ) -> usize {
        if lan.role != Role::Host || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        // Same recovery channel as the graphical host when a DESC was lost.
        for (id, wants) in lan.take_wants() {
            for wanted in wants.into_iter().take(MAX_WANTS_PER_CLIENT) {
                if !wanted.person {
                    continue;
                }
                if let Some(ty) = people.lan_people_by_id(wanted.id) {
                    lan.send_desc(
                        id,
                        &Desc::Person {
                            id: wanted.id,
                            file: PeopleSim::type_file(&ty),
                        },
                    );
                }
            }
        }
        self.elapsed += dt;
        if self.elapsed < world::MOVING_EVERY {
            return 0;
        }
        self.elapsed = (self.elapsed - world::MOVING_EVERY).min(world::MOVING_EVERY);
        let clients: Vec<_> = lan
            .peers()
            .filter(|peer| peer.has_info && peer.has_pose && peer.pose.has_vehicle())
            .filter_map(|peer| {
                let at = DVec3::new(peer.pose.x, peer.pose.y, peer.pose.z);
                (at.is_finite() && at.x.abs() < 1.0e8 && at.y.abs() < 1.0e8)
                    .then_some((peer.pose.id, at))
            })
            .collect();
        let live: HashSet<u32> = clients.iter().map(|(id, _)| *id).collect();
        self.clients.retain(|id, _| live.contains(id));
        let mut bytes = 0;
        for (id, near) in clients {
            let view = self.clients.entry(id).or_default();
            let nearby = people.lan_people(near, PERSON_RADIUS);
            let visible: Vec<_> = nearby
                .iter()
                .filter_map(|person| {
                    let state = state_of(person, table)?;
                    if view.described.insert(state.id) {
                        lan.send_desc(
                            id,
                            &Desc::Person {
                                id: state.id,
                                file: PeopleSim::type_file(&person.ty),
                            },
                        );
                    }
                    Some(state)
                })
                .take(MAX_PEOPLE_PER_CLIENT)
                .collect();
            let frame = view.frame(&visible, world::MOVING_EVERY);
            if !frame.people.is_empty() || !frame.gone.is_empty() {
                bytes += lan.send_world(id, &frame);
            }
        }
        self.sent_bytes += bytes as u64;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: u32, x: f64) -> PersonState {
        PersonState {
            id,
            activity: Activity::Walk,
            place: PersonPlace::Foot {
                x,
                y: 3.0,
                z: 0.0,
                heading: 90.0,
                speed: 1.0,
                waiting: None,
            },
        }
    }

    #[test]
    fn late_join_gets_first_snapshot_and_only_changed_poses_follow() {
        let mut view = ClientPeople::default();
        assert_eq!(
            view.frame(&[person(12, 2.0)], 0.1).people,
            vec![person(12, 2.0)]
        );
        assert!(view.frame(&[person(12, 2.0)], 0.1).people.is_empty());
        assert_eq!(
            view.frame(&[person(12, 4.0)], 0.1).people,
            vec![person(12, 4.0)]
        );
    }

    #[test]
    fn leaving_range_sends_a_single_removal_and_clears_description() {
        let mut view = ClientPeople::default();
        view.described.insert(12);
        view.frame(&[person(12, 2.0)], 0.1);
        assert_eq!(view.frame(&[], 0.1).gone, vec![(true, 12)]);
        assert!(view.frame(&[], 0.1).gone.is_empty());
        assert!(!view.described.contains(&12));
        assert_eq!(view.frame(&[person(12, 2.0)], 0.1).people.len(), 1);
    }

    #[test]
    fn removals_are_bounded_to_the_wire_protocol_limit() {
        let mut view = ClientPeople::default();
        let people: Vec<_> = (0..90).map(|id| person(id, id as f64)).collect();
        view.frame(&people, 0.1);
        assert_eq!(view.frame(&[], 0.1).gone.len(), MAX_GONE_PER_FRAME);
        assert_eq!(view.frame(&[], 0.1).gone.len(), 90 - MAX_GONE_PER_FRAME);
    }
}

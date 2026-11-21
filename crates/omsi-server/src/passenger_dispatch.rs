//! Native host passenger coordination for multiple player-operated buses.
//!
//! A client already sends its onboard count through Pose.passengers. GRANT
//! transfers ownership of a waiting person, but is NOT proof they boarded.
//! Keep these measurements separate and split competing claims at the same
//! physical stop among buses advertising the same line and destination.
//!
//! This is intentionally conservative. A display line is not an authenticated
//! timetable duty or a route graph; never fabricate boarding or rewrite the
//! player's locally executed bus script from this bookkeeping.

use glam::DVec3;
use omsi_net::{LanSession, Pose};
use omsi_sim::people::{HandedPerson, PeopleSim};
use std::collections::{HashMap, HashSet};

const SAME_STOP_RADIUS: f64 = 60.0;
const SAME_STOP_VERTICAL: f64 = 10.0;
const MAX_WAITING_PER_TICK: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Service {
    line: String,
    destination: String,
}

fn normalized(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl Service {
    fn of(p: &Pose) -> Option<Self> {
        // A tour is <line>/<tour>; a sign is not necessarily set when a
        // player first takes duty. Group only vehicles with a real line.
        let line = normalized(if p.line.trim().is_empty() {
            p.tour.split('/').next().unwrap_or("")
        } else {
            &p.line
        });
        if line.is_empty() || line == "-" || line == "0" {
            return None;
        }
        Some(Self {
            line,
            destination: normalized(&p.destination),
        })
    }
}

#[derive(Debug, Clone)]
struct Driver {
    id: u32,
    connection: u64,
    bus: String,
    service: Option<Service>,
    position: DVec3,
    onboard: u32,
    heading: f32,
    boarded: u64,
    alighted: u64,
    stopped: bool,
    doors_open: bool,
}

impl Driver {
    fn from_pose(id: u32, connection: u64, pose: &Pose) -> Option<Self> {
        if !pose.has_vehicle() {
            return None;
        }
        let position = crate::interest::client_position(pose)?;
        Some(Self {
            id,
            connection,
            bus: pose.bus.clone(),
            service: Service::of(pose),
            position,
            onboard: pose.passengers,
            heading: pose.heading,
            boarded: 0,
            alighted: 0,
            stopped: pose.speed_kmh.is_finite() && pose.speed_kmh.abs() <= 5.0,
            doors_open: pose.doors.is_empty()
                || pose.doors.iter().any(|door| door.is_finite() && *door > 0.1),
        })
    }

    fn near(&self, stop: DVec3) -> bool {
        (self.position - stop).truncate().length() <= SAME_STOP_RADIUS
            && (self.position.z - stop.z).abs() <= SAME_STOP_VERTICAL
    }

    /// A bus driving through a stop (or parked with shut doors) cannot
    /// pre-claim its waiting people. Empty door lists are legacy/unknown;
    /// they may claim only while stopped, never while driving through.
    fn boarding_at(&self, stop: DVec3) -> bool {
        self.near(stop) && self.stopped && self.doors_open
    }

    fn same_direction(&self, other: &Self) -> bool {
        (f64::from(self.heading) - f64::from(other.heading))
            .to_radians()
            .cos()
            >= 0.5
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct StopService {
    stop: i64,
    service: Service,
}

#[derive(Debug)]
struct Member {
    connection: u64,
    bus: String,
    baseline: u32,
    granted: usize,
}

#[derive(Default)]
struct Visit {
    position: DVec3,
    members: HashMap<u32, Member>,
}

struct EscrowEntry {
    connection: u64,
    person: HandedPerson,
}

/// Owns only player-reported occupancy and temporary stop allocations.
/// The PeopleSim host remains authoritative for which person still waits.
#[derive(Default)]
pub(crate) struct PassengerDispatch {
    drivers: HashMap<u32, Driver>,
    visits: HashMap<StopService, Visit>,
    /// Original server-owned people, pending the client's explicit physical
    /// boarding confirmation or cancellation. A GRANT alone is not boarding.
    escrow: HashMap<u32, HashMap<u32, EscrowEntry>>,
}

impl PassengerDispatch {
    pub(crate) fn observe(&mut self, lan: &LanSession) {
        let drivers: Vec<_> = lan
            .peers()
            .filter(|peer| peer.has_pose && peer.has_info)
            .filter_map(|peer| Driver::from_pose(peer.pose.id, peer.conn, &peer.pose))
            .collect();
        self.update(drivers);
    }

    fn update(&mut self, drivers: Vec<Driver>) {
        let old = std::mem::take(&mut self.drivers);
        for mut driver in drivers {
            if let Some(previous) = old.get(&driver.id).filter(|previous| {
                previous.connection == driver.connection && previous.bus == driver.bus
            }) {
                driver.boarded = previous
                    .boarded
                    .saturating_add(u64::from(driver.onboard.saturating_sub(previous.onboard)));
                driver.alighted = previous
                    .alighted
                    .saturating_add(u64::from(previous.onboard.saturating_sub(driver.onboard)));
            }
            self.drivers.insert(driver.id, driver);
        }
        // End the visit when the bus leaves, changes line, changes vehicle or
        // disconnects. A reconnect never inherits a prior driver's grant share.
        self.visits.retain(|key, visit| {
            visit.members.retain(|id, member| {
                self.drivers.get(id).is_some_and(|driver| {
                    driver.connection == member.connection
                        && driver.bus == member.bus
                        && driver.service.as_ref() == Some(&key.service)
                        && driver.boarding_at(visit.position)
                })
            });
            !visit.members.is_empty()
        });
        // A returning LAN player may receive the same numeric id with a
        // different peer connection. Never transfer an old rider escrow to
        // that new connection (or let it return someone else's old claim).
        self.escrow.retain(|player, held| {
            held.retain(|_, entry| {
                self.drivers
                    .get(player)
                    .is_some_and(|driver| driver.connection == entry.connection)
            });
            !held.is_empty()
        });
    }

    pub(crate) fn totals(&self) -> (usize, u64, u64, u64) {
        (
            self.drivers.len(),
            self.drivers.values().map(|d| u64::from(d.onboard)).sum(),
            self.drivers.values().map(|d| d.boarded).sum(),
            self.drivers.values().map(|d| d.alighted).sum(),
        )
    }

    /// Session-local per-bus occupancy. IDs are LAN player numbers, not names.
    pub(crate) fn per_bus(&self) -> Vec<(u32, u32, u64, u64)> {
        let mut buses: Vec<_> = self
            .drivers
            .values()
            .map(|driver| (driver.id, driver.onboard, driver.boarded, driver.alighted))
            .collect();
        buses.sort_unstable_by_key(|row| row.0);
        buses
    }

    /// Filter a batch of eligible host passenger IDs before PeopleSim hands
    /// them over. `waiting` contains only people waiting at an identified
    /// stop. A person who has no stop still uses the legacy individual claim.
    pub(crate) fn choose(
        &mut self,
        player: u32,
        requests: &[u32],
        waiting: &HashMap<u32, i64>,
        counts: &HashMap<i64, usize>,
        stop_positions: &HashMap<i64, DVec3>,
    ) -> Vec<u32> {
        let Some(owner) = self.drivers.get(&player) else {
            return Vec::new();
        };
        // Even without a usable stop ID, a moving bus must not pre-claim
        // people and make them vanish before the next service arrives.
        if !owner.stopped || !owner.doors_open {
            return Vec::new();
        }
        let service = owner.service.clone();
        let mut chosen = Vec::new();
        let mut chosen_per_group: HashMap<StopService, usize> = HashMap::new();
        let mut seen = HashSet::new();

        for &id in requests.iter().take(MAX_WAITING_PER_TICK) {
            if !seen.insert(id) {
                continue;
            }
            let Some(&stop) = waiting.get(&id) else {
                // A waiting passenger with no stop record cannot be grouped
                // by service, but legacy hand_over still validates its state.
                chosen.push(id);
                continue;
            };
            let Some(&position) = stop_positions.get(&stop) else {
                chosen.push(id);
                continue;
            };
            // A claimant who skipped this stop must not take passengers
            // away from the server; the following bus can still claim them.
            if !owner.boarding_at(position) {
                continue;
            }
            let Some(service) = service.as_ref() else {
                chosen.push(id);
                continue;
            };
            let key = StopService {
                stop,
                service: service.clone(),
            };
            let candidates: Vec<_> = self
                .drivers
                .values()
                .filter(|d| {
                    d.service.as_ref() == Some(service)
                        && d.boarding_at(position)
                        && d.same_direction(owner)
                })
                .collect();
            if candidates.len() <= 1 {
                chosen.push(id);
                continue;
            }
            let visit = self.visits.entry(key.clone()).or_insert_with(|| Visit {
                position,
                members: HashMap::new(),
            });
            let eligible: HashSet<u32> = candidates.iter().map(|d| d.id).collect();
            visit.members.retain(|id, _| eligible.contains(id));
            for driver in candidates {
                visit.members.entry(driver.id).or_insert_with(|| Member {
                    connection: driver.connection,
                    bus: driver.bus.clone(),
                    baseline: driver.onboard,
                    granted: 0,
                });
            }
            let members = visit.members.len();
            let demand = counts
                .get(&stop)
                .copied()
                .unwrap_or(0)
                .saturating_add(visit.members.values().map(|m| m.granted).sum::<usize>());
            let initial_load = visit
                .members
                .values()
                .map(|m| m.baseline as usize)
                .sum::<usize>();
            let equalized = initial_load.saturating_add(demand).div_ceil(members);
            let Some(my) = visit.members.get(&player) else {
                continue;
            };
            let provisional = chosen_per_group.entry(key).or_default();
            let entitlement = equalized.saturating_sub(my.baseline as usize);
            if my.granted.saturating_add(*provisional) < entitlement {
                *provisional += 1;
                chosen.push(id);
            }
        }
        chosen
    }

    /// Retain original, server-owned waiting people after GRANT. The client
    /// can return only its own IDs, never someone else's passengers.
    pub(crate) fn escrow_granted(&mut self, player: u32, granted: Vec<HandedPerson>) {
        let Some(connection) = self.drivers.get(&player).map(|driver| driver.connection) else {
            return;
        };
        let holding = self.escrow.entry(player).or_default();
        for person in granted {
            holding.insert(person.person.id, EscrowEntry { connection, person });
        }
    }

    /// The client has confirmed crossing into its vehicle. Discard only
    /// escrow entries actually owned by this peer; duplicates are idempotent.
    pub(crate) fn confirm_boarded(&mut self, player: u32, ids: &[u32]) {
        let Some(connection) = self.drivers.get(&player).map(|driver| driver.connection) else {
            return;
        };
        let Some(holding) = self.escrow.get_mut(&player) else {
            return;
        };
        for id in ids {
            if holding.get(id).is_some_and(|entry| entry.connection == connection) {
                holding.remove(id);
            }
        }
        if holding.is_empty() {
            self.escrow.remove(&player);
        }
    }

    /// A client cancelled a GRANT while its person was still outside the bus.
    /// Return them to their original stop once only, keeping stable IDs.
    /// Failure to restore (e.g. streamed-out stop) is not acknowledged and
    /// retains the escrow so the client can retry later.
    pub(crate) fn return_unboarded(
        &mut self,
        player: u32,
        ids: &[u32],
        people: &mut PeopleSim,
    ) -> Vec<u32> {
        let mut acknowledged = Vec::new();
        let connection = self.drivers.get(&player).map(|driver| driver.connection);
        for id in ids {
            let entry = self
                .escrow
                .get_mut(&player)
                .and_then(|holding| holding.remove(id));
            match entry {
                Some(entry) if Some(entry.connection) == connection => {
                    // Do not throw away the original person if their stop was
                    // removed by streaming or a Chrono map swap.
                    if !people.stops.contains_key(
                        &entry.person.stop.unwrap_or(i64::MIN),
                    ) {
                        self.escrow.entry(player).or_default().insert(*id, entry);
                        continue;
                    }
                    if people.restore_unboarded(entry.person) {
                        acknowledged.push(*id);
                    }
                }
                Some(entry) => {
                    self.escrow.entry(player).or_default().insert(*id, entry);
                }
                None => acknowledged.push(*id), // repeated UDP return
            }
        }
        if self.escrow.get(&player).is_some_and(HashMap::is_empty) {
            self.escrow.remove(&player);
        }
        acknowledged
    }

    /// Record GRANT, not an actual boarding. Occupancy changes only when the
    /// client's next Pose.passengers update arrives.
    pub(crate) fn granted(
        &mut self,
        player: u32,
        granted: &[u32],
        waiting: &HashMap<u32, i64>,
    ) {
        let Some(service) = self.drivers.get(&player).and_then(|d| d.service.clone()) else {
            return;
        };
        for id in granted {
            let Some(&stop) = waiting.get(id) else { continue };
            let key = StopService {
                stop,
                service: service.clone(),
            };
            if let Some(member) = self
                .visits
                .get_mut(&key)
                .and_then(|visit| visit.members.get_mut(&player))
            {
                member.granted = member.granted.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver(id: u32, count: u32, destination: &str) -> Driver {
        Driver {
            id,
            connection: id as u64,
            bus: format!("Vehicles/{id}.bus"),
            service: Some(Service {
                line: "233".into(),
                destination: destination.into(),
            }),
            position: DVec3::new(10.0, 0.0, 0.0),
            onboard: count,
            heading: 0.0,
            boarded: 0,
            alighted: 0,
            stopped: true,
            doors_open: true,
        }
    }

    fn sample() -> (HashMap<u32, i64>, HashMap<i64, usize>, HashMap<i64, DVec3>) {
        (
            (1..=10).map(|id| (id, 70)).collect(),
            HashMap::from([(70, 10)]),
            HashMap::from([(70, DVec3::ZERO)]),
        )
    }

    #[test]
    fn reports_client_onboard_count_separately_from_claim_grants() {
        let mut dispatch = PassengerDispatch::default();
        dispatch.update(vec![driver(1, 0, "terminal")]);
        dispatch.update(vec![driver(1, 6, "terminal")]);
        dispatch.update(vec![driver(1, 4, "terminal")]);
        assert_eq!(dispatch.totals(), (1, 4, 6, 2));
        dispatch.update(vec![]);
        assert_eq!(dispatch.totals(), (0, 0, 0, 0));
    }

    #[test]
    fn divides_claimed_people_between_two_buses_on_same_service() {
        let mut dispatch = PassengerDispatch::default();
        dispatch.update(vec![driver(1, 0, "terminal"), driver(2, 0, "terminal")]);
        let (mut waiting, mut counts, stops) = sample();
        let requested: Vec<_> = (1..=10).collect();
        let first = dispatch.choose(1, &requested, &waiting, &counts, &stops);
        assert_eq!(first.len(), 5);
        dispatch.granted(1, &first, &waiting);
        for id in &first {
            waiting.remove(id);
        }
        counts.insert(70, 5);
        let still_waiting: Vec<u32> = requested
            .into_iter()
            .filter(|id| waiting.contains_key(id))
            .collect();
        let second = dispatch.choose(2, &still_waiting, &waiting, &counts, &stops);
        assert_eq!(second.len(), 5);
        assert!(second.iter().all(|id| !first.contains(id)));
    }

    #[test]
    fn a_bus_can_serve_the_remaining_stop_after_its_competitor_disconnects() {
        let mut dispatch = PassengerDispatch::default();
        dispatch.update(vec![driver(1, 0, "terminal"), driver(2, 0, "terminal")]);
        let (mut waiting, mut counts, stops) = sample();
        let all: Vec<u32> = (1..=10).collect();
        let first = dispatch.choose(1, &all, &waiting, &counts, &stops);
        assert_eq!(first.len(), 5);
        dispatch.granted(1, &first, &waiting);
        for id in &first {
            waiting.remove(id);
        }
        counts.insert(70, 5);
        dispatch.update(vec![driver(1, 0, "terminal")]);
        let pending: Vec<_> = all
            .into_iter()
            .filter(|id| waiting.contains_key(id))
            .collect();
        assert_eq!(
            dispatch.choose(1, &pending, &waiting, &counts, &stops),
            pending
        );
    }

    #[test]
    fn skipping_a_stop_does_not_take_people_from_the_next_bus() {
        let mut dispatch = PassengerDispatch::default();
        let mut first = driver(1, 0, "terminal");
        first.stopped = false; // driving past without serving the stop
        first.doors_open = false;
        let mut next = driver(2, 0, "terminal");
        next.stopped = false; // has not pulled up yet
        next.doors_open = false;
        dispatch.update(vec![first.clone(), next.clone()]);
        let (waiting, counts, stops) = sample();
        let all: Vec<_> = (1..=10).collect();
        assert!(dispatch.choose(1, &all, &waiting, &counts, &stops).is_empty());
        // No GRANT occurred: the host's same ten waiting people remain.
        first.position = DVec3::new(100.0, 0.0, 0.0);
        next.stopped = true;
        next.doors_open = true;
        dispatch.update(vec![first, next]);
        assert_eq!(dispatch.choose(2, &all, &waiting, &counts, &stops), all);
    }

    #[test]
    fn closing_doors_and_departing_releases_an_unused_boarding_share() {
        let mut dispatch = PassengerDispatch::default();
        let first = driver(1, 0, "terminal");
        let second = driver(2, 0, "terminal");
        dispatch.update(vec![first.clone(), second.clone()]);
        let (waiting, counts, stops) = sample();
        let all: Vec<_> = (1..=10).collect();
        assert_eq!(dispatch.choose(1, &all, &waiting, &counts, &stops).len(), 5);
        assert_eq!(dispatch.visits.len(), 1);

        let mut departed = first;
        departed.stopped = false;
        departed.doors_open = false;
        dispatch.update(vec![departed, second]);
        // The skipped driver's reservation disappears, but the next
        // bus's active visit is kept so its real boarding can proceed.
        assert!(dispatch.visits.values().all(|visit| !visit.members.contains_key(&1)));
        assert_eq!(dispatch.choose(2, &all, &waiting, &counts, &stops), all);
    }

    #[test]
    fn closed_doors_do_not_hold_a_share_against_a_bus_that_opens() {
        let mut dispatch = PassengerDispatch::default();
        let mut bypassing = driver(1, 0, "terminal");
        bypassing.doors_open = false;
        let serving = driver(2, 0, "terminal");
        dispatch.update(vec![bypassing, serving]);
        let (waiting, counts, stops) = sample();
        let all: Vec<_> = (1..=10).collect();
        assert!(dispatch.choose(1, &all, &waiting, &counts, &stops).is_empty());
        assert_eq!(dispatch.choose(2, &all, &waiting, &counts, &stops), all);
    }

    #[test]
    fn lighter_bus_receives_more_when_two_buses_share_a_line() {
        let mut dispatch = PassengerDispatch::default();
        dispatch.update(vec![driver(1, 10, "terminal"), driver(2, 0, "terminal")]);
        let (waiting, counts, stops) = sample();
        assert!(dispatch
            .choose(1, &(1..=10).collect::<Vec<_>>(), &waiting, &counts, &stops)
            .is_empty());
        assert_eq!(
            dispatch.choose(2, &(1..=10).collect::<Vec<_>>(), &waiting, &counts, &stops).len(),
            10
        );
    }

    #[test]
    fn different_directions_and_departed_buses_do_not_reserve_passengers() {
        let mut dispatch = PassengerDispatch::default();
        dispatch.update(vec![driver(1, 0, "terminal a"), driver(2, 0, "terminal b")]);
        let (waiting, counts, stops) = sample();
        assert_eq!(
            dispatch
                .choose(1, &(1..=10).collect::<Vec<_>>(), &waiting, &counts, &stops)
                .len(),
            10
        );
        dispatch.update(vec![driver(1, 0, "terminal a")]);
        assert_eq!(dispatch.totals().0, 1);
        assert_eq!(
            dispatch
                .choose(1, &(1..=10).collect::<Vec<_>>(), &waiting, &counts, &stops)
                .len(),
            10
        );
    }

    #[test]
    fn opposite_running_direction_does_not_split_a_platform() {
        let mut dispatch = PassengerDispatch::default();
        let first = driver(1, 0, "terminal");
        let mut opposite = driver(2, 0, "terminal");
        opposite.heading = 180.0;
        dispatch.update(vec![first, opposite]);
        let (waiting, counts, stops) = sample();
        assert_eq!(
            dispatch
                .choose(1, &(1..=10).collect::<Vec<_>>(), &waiting, &counts, &stops)
                .len(),
            10
        );
    }

    #[test]
    fn existing_pose_fields_identify_service_and_reported_occupancy() {
        let pose = Pose {
            bus: "Vehicles/Bus.bus".into(),
            flags: omsi_net::FLAG_VEHICLE,
            tour: "233/circular-1".into(),
            destination: "  Terminal   Norte ".into(),
            passengers: 18,
            ..Default::default()
        };
        let reading = Driver::from_pose(3, 10, &pose).unwrap();
        assert_eq!(reading.onboard, 18);
        assert_eq!(reading.service.unwrap().line, "233");
        assert_eq!(normalized("  Terminal   Norte  "), "terminal norte");
    }
}

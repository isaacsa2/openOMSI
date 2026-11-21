use omsi_net::Footprint;

const MARGIN: f64 = 1.5;

pub fn inside(f: &Footprint, local: [f32; 3]) -> bool {
    let (across, along) = (local[0] as f64, local[1] as f64);
    across.abs() <= f.width as f64 * 0.5 + MARGIN && along.abs() <= f.length as f64 + MARGIN
}

pub fn passenger_trigger(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_suffix("_off").unwrap_or(&n);
    n.starts_with("int_") || n.contains("haltewunsch") || n.contains("kinderwagenwunsch")
}

/// May player `from` work switch `ev` of our bus `bus` (`lan`: our session)? Only a passenger
/// aboard our bus, inside it, and only a switch passengers work (`passenger_trigger`).
pub fn may_work(
    lan: &omsi_net::LanSession,
    from: u32,
    bus: Option<&omsi_sim::VehicleInstance>,
    ev: &str,
) -> bool {
    let walker = lan
        .peers()
        .find(|p| p.pose.id == from)
        .and_then(|p| p.pose.walker);
    let aboard = walker
        .and_then(|w| w.aboard)
        .filter(|a| a.owner == lan.my_id);
    let inside = match (aboard, bus) {
        (Some(a), Some(v)) => inside(
            &crate::spawn::footprint_of(v, crate::spawn::BUS_BOX),
            a.local,
        ),
        _ => false,
    };
    aboard.is_some() && inside && passenger_trigger(ev)
}

/// Whether a pending boarding grant should be returned after the owner's bus
/// leaves the stop. This is pure LAN policy, independent of game rendering.
pub fn left_boarding_stop(bus: Option<(glam::DVec3, f64)>, stop: Option<glam::DVec3>) -> bool {
    match (bus, stop) {
        (Some((position, speed)), Some(point)) => {
            let distance = (position - point).truncate().length();
            distance > 75.0 || (distance > 25.0 && speed.abs() > 2.0)
        }
        (None, _) => true,
        _ => false,
    }
}

/// Resend physical BOARD/RETURN reports until the authoritative host ACKs;
/// a dropped UDP acknowledgement must not lose a rider.
pub fn resend_until_ack(
    lan: &mut omsi_net::LanSession,
    pending: &mut hashbrown::HashMap<u32, Option<std::time::Instant>>,
    now: std::time::Instant,
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

/// Coalesce bursty passenger requests into one bounded UDP CLAIM message.
/// Individual frames can discover waiting people in several groups, while the
/// host also rate-limits INFO and PAXBOARD/RETURN messages from this peer.
#[derive(Default)]
pub struct ClaimQueue {
    pending: std::collections::BTreeSet<u32>,
    last_sent: Option<std::time::Instant>,
}

impl ClaimQueue {
    const INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
    const MAX_BATCH: usize = 64;

    /// Clear claims belonging to the previous host/session.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.last_sent = None;
    }

    fn take_due(
        &mut self,
        requested: impl IntoIterator<Item = u32>,
        now: std::time::Instant,
    ) -> Vec<u32> {
        self.pending.extend(requested);
        if self.pending.is_empty()
            || self
                .last_sent
                .is_some_and(|last| now.saturating_duration_since(last) < Self::INTERVAL)
        {
            return Vec::new();
        }
        let due: Vec<u32> = self.pending.iter().take(Self::MAX_BATCH).copied().collect();
        for id in &due {
            self.pending.remove(id);
        }
        self.last_sent = Some(now);
        due
    }

    pub fn send_due(
        &mut self,
        lan: &omsi_net::LanSession,
        requested: impl IntoIterator<Item = u32>,
        now: std::time::Instant,
    ) -> usize {
        let due = self.take_due(requested, now);
        if !due.is_empty() {
            lan.claim(&due);
        }
        due.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passenger_is_inside_the_bus_it_claims() {
        let bus = Footprint {
            x: 100.0,
            y: 50.0,
            z: 0.0,
            heading: 90.0,
            length: 12.0,
            width: 2.5,
        };
        assert!(inside(&bus, [0.0, 0.0, 1.0]));
        assert!(inside(&bus, [0.8, -5.5, 1.0]));
        assert!(!inside(&bus, [5.0, 0.0, 1.0]));
        assert!(!inside(&bus, [0.0, 200.0, 1.0]));
    }

    #[test]
    fn claims_are_deduplicated_and_sent_at_most_twice_per_second() {
        let mut queue = ClaimQueue::default();
        let now = std::time::Instant::now();
        let first = queue.take_due(0..100, now);
        assert_eq!(first.len(), 64);
        assert_eq!(first[0], 0);
        assert_eq!(first[63], 63);
        // A second group found in the same frame must not flood the
        // authenticated host's 10-message-per-second bucket.
        assert!(
            queue
                .take_due(50..150, now + std::time::Duration::from_millis(200))
                .is_empty()
        );
        let second = queue.take_due(
            std::iter::empty(),
            now + std::time::Duration::from_millis(500),
        );
        assert_eq!(second, (64..128).collect::<Vec<_>>());
        let third = queue.take_due(
            std::iter::empty(),
            now + std::time::Duration::from_millis(1_000),
        );
        assert_eq!(third, (128..150).collect::<Vec<_>>());
        queue.clear();
        assert!(
            queue
                .take_due(
                    std::iter::empty(),
                    now + std::time::Duration::from_millis(1_500),
                )
                .is_empty()
        );
    }

    #[test]
    fn boarding_retries_accept_the_client_hashbrown_pending_ledger() {
        // Compile-time regression for the exact HashMap implementation used
        // by omsi-app::lan_world, which differs from std::collections::HashMap.
        fn accepts_client_ledger(_: &mut hashbrown::HashMap<u32, Option<std::time::Instant>>) {}
        let mut pending = hashbrown::HashMap::new();
        pending.insert(7, None);
        accepts_client_ledger(&mut pending);
    }

    #[test]
    fn boarding_grant_returns_only_after_bus_leaves_a_stop() {
        let stop = Some(glam::DVec3::ZERO);
        assert!(!left_boarding_stop(Some((glam::DVec3::ZERO, 0.0)), stop));
        assert!(!left_boarding_stop(
            Some((glam::DVec3::new(26.0, 0.0, 0.0), 1.0)),
            stop
        ));
        assert!(left_boarding_stop(
            Some((glam::DVec3::new(26.0, 0.0, 0.0), 3.0)),
            stop
        ));
        assert!(left_boarding_stop(
            Some((glam::DVec3::new(80.0, 0.0, 0.0), 0.0)),
            stop
        ));
        assert!(left_boarding_stop(None, stop));
        assert!(!left_boarding_stop(Some((glam::DVec3::ZERO, 0.0)), None));
    }

    #[test]
    fn only_a_passengers_controls_are_worked_from_another_game() {
        assert!(passenger_trigger("int_haltewunsch"));
        assert!(passenger_trigger("Haltewunsch_off"));
        assert!(passenger_trigger("int_kinderwagenwunsch"));
        assert!(!passenger_trigger("bus_doorfront0"));
        assert!(!passenger_trigger("handbrake"));
        assert!(!passenger_trigger("engine_start"));
    }
}

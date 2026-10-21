//! Opt-in AI signal diagnostics. Observe what the simulation decided; never change it.
//! The normal game does no work here besides checking the debug flag.

use super::*;

#[derive(Clone, Copy)]
struct Seen {
    lane: usize,
    last_report: f32,
}

/// Per-car observation, limited to the vehicles currently in the simulation.
#[derive(Default)]
pub struct SignalDecisions {
    seen: HashMap<u64, Seen>,
    cleanup_at: f32,
}

/// A standing car at a proceed aspect may be blocked by a lead, a junction
/// reservation, an exit, a pedestrian or its timetable rather than the light.
fn stopped_at_proceed(aspect: Aspect, speed: f32, stopped: f32, gap: f32) -> bool {
    matches!(aspect, Aspect::Green | Aspect::Dark)
        && speed < 0.5
        && stopped >= 3.0
        && (-3.0..=40.0).contains(&gap)
}

impl TrafficSim {
    /// Diagnose red entries and cars standing at a green signal. Log only with
    /// OMSI_DEBUG_SIGNAL_DECISIONS, rate-limiting stopped cars to one line / 5 sim s.
    pub(super) fn trace_signal_decisions(&mut self) {
        if !omsi_cfg::flags::OMSI_DEBUG_SIGNAL_DECISIONS.is_set() {
            return;
        }
        let now = self.time;
        for c in &self.cars {
            let lane = c.state.lane;
            if c.gone || lane >= self.net.lanes.len() {
                continue;
            }
            let previous = self.signal_decisions.seen.get(&c.id).copied();
            if let Some(prev) = previous.filter(|old| old.lane != lane && old.lane < self.net.lanes.len()) {
                // Use the same entry-light association as AI stop decisions and
                // OMSI_TRAFFIC_STATS; entering on red is a *candidate*, since an
                // amber commitment or emergency priority may legitimately allow it.
                let segment = [(prev.lane, 0.0), (lane, 0.0)];
                if let Some((ci, li)) = entry_light(&self.net, &segment, 1) {
                    if let Some(ctl) = self.lights.get(ci) {
                        let phase = ctl.state(li);
                        if matches!(TrafficLightController::aspect(phase), Aspect::Red | Aspect::RedYellow) {
                            log::info!(
                                "AI SIGNAL red-entry candidate: t={:.2} car={} lane={} -> {} signal={}/{} phase={} amber={:?} emergency={} speed={:.2}m/s reason={:?} held={} pos=({:.1},{:.1})",
                                now, c.id, prev.lane, lane, ci, li, phase, c.amber,
                                self.emergency.crawling(c.id), c.state.speed, c.why,
                                c.light_hold, c.vehicle.position.x, c.vehicle.position.y
                            );
                        }
                    }
                }
            }

            let mut last_report = previous.map(|old| old.last_report).unwrap_or(f32::NEG_INFINITY);
            if c.state.speed < 0.5 && c.stopped >= 3.0 && now - last_report >= 5.0 {
                // Only ask for the path ahead when this car is a possible stalled
                // vehicle; normal non-debug driving does not search again.
                let way = self.way_lanes(&c.state, 70.0);
                let first = way.iter().enumerate().skip(1).filter_map(|(k, &(_, d))| {
                    let gap = d - c.state.front;
                    if !(-3.0..=40.0).contains(&gap) {
                        return None;
                    }
                    let (ci, li) = entry_light(&self.net, &way, k)?;
                    let ctl = self.lights.get(ci)?;
                    let phase = ctl.state(li);
                    Some((gap, ci, li, phase, ctl.request.get(li).copied().unwrap_or(false), ctl.held))
                }).next();
                if let Some((gap, ci, li, phase, request, program_held)) = first {
                    let aspect = TrafficLightController::aspect(phase);
                    if stopped_at_proceed(aspect, c.state.speed, c.stopped, gap) {
                        log::info!(
                            "AI SIGNAL stopped-at-proceed: t={:.2} car={} bus={} lane={} signal={}/{} phase={} aspect={:?} gap={:.1}m request={} program_held={} speed={:.2}m/s stood={:.1}s reason={:?} light_stop={:?} yielding={} yield_stop={:?} lead={:?} reserved={} junction={:?} pos=({:.1},{:.1})",
                            now, c.id, c.is_bus(), lane, ci, li, phase, aspect, gap,
                            request, program_held, c.state.speed, c.stopped, c.why,
                            c.light_at, c.yielding, c.wait_at, c.lead_info,
                            c.reserved.len(), c.junction_why, c.vehicle.position.x, c.vehicle.position.y
                        );
                        last_report = now;
                    }
                }
            }
            self.signal_decisions.seen.insert(c.id, Seen { lane, last_report });
        }
        if now >= self.signal_decisions.cleanup_at {
            // A long drive should not keep a record for every despawned AI car.
            let alive: hashbrown::HashSet<u64> = self.cars.iter().map(|c| c.id).collect();
            self.signal_decisions.seen.retain(|id, _| alive.contains(id));
            self.signal_decisions.cleanup_at = now + 30.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stationary_cars_near_a_proceed_signal_are_reported() {
        assert!(stopped_at_proceed(Aspect::Green, 0.0, 5.0, 12.0));
        assert!(stopped_at_proceed(Aspect::Dark, 0.1, 3.0, 0.0));
        assert!(!stopped_at_proceed(Aspect::Red, 0.0, 5.0, 12.0));
        assert!(!stopped_at_proceed(Aspect::Green, 5.0, 5.0, 12.0));
        assert!(!stopped_at_proceed(Aspect::Green, 0.0, 1.0, 12.0));
        assert!(!stopped_at_proceed(Aspect::Green, 0.0, 5.0, 80.0));
    }
}

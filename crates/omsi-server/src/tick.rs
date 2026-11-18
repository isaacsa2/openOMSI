//! Fixed-rate tick pacing for a future standalone headless server.
//!
//! Simulation always advances with FIXED_DT; sleeping is a separate wall-clock
//! concern. After a long OS pause, discard obsolete deadlines rather than
//! executing hundreds of back-to-back simulation steps or changing physics dt.

use std::time::{Duration, Instant};

pub const SERVER_TPS: u32 = 60;
pub const FIXED_DT: f32 = 1.0 / SERVER_TPS as f32;
pub const TICK_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / SERVER_TPS as u64 + 1);
const MAX_CATCHUP_STEPS: u128 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickSchedule {
    /// Sleep before starting the next simulation step.
    pub wait: Duration,
    /// Deadlines skipped due to a long pause or tick overrun.
    pub discarded_deadlines: u64,
}

/// One nominal 60 Hz timeline, independent of traffic density and rendering.
pub struct TickPacer {
    next_due: Instant,
}

impl TickPacer {
    pub fn new(now: Instant) -> Self {
        Self { next_due: now }
    }

    /// Call immediately after each completed fixed-dt tick. The returned
    /// duration may be zero when the process is briefly catching up.
    /// Never turn elapsed wall-clock delay into a larger physics time step.
    pub fn after_tick(&mut self, now: Instant) -> TickSchedule {
        let due = self.next_due + TICK_PERIOD;
        let late = now.saturating_duration_since(due);
        let late_slots = late.as_nanos() / TICK_PERIOD.as_nanos();

        if late_slots > MAX_CATCHUP_STEPS {
            // A stalled VPS may have missed seconds or minutes. Discard stale
            // scheduling deadlines, but do not alter the simulated time.
            self.next_due = now + TICK_PERIOD;
            return TickSchedule {
                wait: TICK_PERIOD,
                discarded_deadlines: u64::try_from(late_slots + 1).unwrap_or(u64::MAX),
            };
        }
        self.next_due = due;
        TickSchedule {
            wait: due.saturating_duration_since(now),
            discarded_deadlines: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_normal_tick_preserves_sixty_hertz_without_accumulating_work() {
        let now = Instant::now();
        let mut pacer = TickPacer::new(now);
        for step in 1..=120 {
            let tick_started = now + TICK_PERIOD * (step - 1);
            let tick_finished = tick_started + Duration::from_millis(4);
            let plan = pacer.after_tick(tick_finished);
            assert_eq!(plan.wait, TICK_PERIOD - Duration::from_millis(4));
            assert_eq!(plan.discarded_deadlines, 0);
        }
    }

    #[test]
    fn a_short_overrun_can_catch_up_without_changing_physics_dt() {
        let now = Instant::now();
        let mut pacer = TickPacer::new(now);
        let delayed = pacer.after_tick(now + TICK_PERIOD * 2);
        assert_eq!(delayed.wait, Duration::ZERO);
        assert_eq!(delayed.discarded_deadlines, 0);
        let next = pacer.after_tick(now + TICK_PERIOD * 2);
        assert_eq!(next.wait, Duration::ZERO);
        assert_eq!(next.discarded_deadlines, 0);
        let recovered = pacer.after_tick(now + TICK_PERIOD * 2);
        assert_eq!(recovered.wait, TICK_PERIOD);
    }

    #[test]
    fn long_pause_drops_old_deadlines_instead_of_simulation_spiral() {
        let now = Instant::now();
        let mut pacer = TickPacer::new(now);
        let paused = pacer.after_tick(now + Duration::from_secs(3));
        assert!(paused.discarded_deadlines >= 170);
        assert_eq!(paused.wait, TICK_PERIOD);
        let next = pacer.after_tick(now + Duration::from_secs(3) + TICK_PERIOD);
        assert_eq!(next.wait, TICK_PERIOD);
        assert_eq!(next.discarded_deadlines, 0);
        assert!((FIXED_DT - 1.0 / 60.0).abs() < f32::EPSILON);
    }
}

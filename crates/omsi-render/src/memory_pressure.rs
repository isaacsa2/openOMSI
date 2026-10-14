//! How full the graphics memory is, as the driver says it during the game, and what the loading
//! of things nobody asked for yet should do about it.
//!
//! The texture allowance is a number taken once at the start (`gpu_memory`). Over an hour of
//! driving the memory in use is what counts: a map's tiles, the fleet read ahead, the render
//! targets and the driver's own share all sit in it. Where the backend reports its budget and
//! the memory the process uses (`Adapter::memory_info`), [`memory_pressure`] says whether there
//! is room (`Calm`), little (`Tight`) or none to speak of (`Critical`). Callers use it to put
//! off what is only done in advance - nothing in use is let go because of it.
//!
//! Off unless `OMSI_MEMORY_PRESSURE` is set. Where nothing is reported it is always `Calm`.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How tight the graphics memory is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pressure {
    Calm,
    /// Little room: no new reading ahead.
    Tight,
    /// None to speak of: what nobody uses goes sooner as well.
    Critical,
}

/// The share of the budget in use at which a level begins, and below which the one under it
/// returns. A gap between the two keeps the level from flickering at the edge.
const TIGHT_UP: f32 = 0.85;
const TIGHT_DOWN: f32 = 0.75;
const CRITICAL_UP: f32 = 0.95;
const CRITICAL_DOWN: f32 = 0.88;
/// How long a level holds before the one under it returns: the way back is one step at a time.
const HOLD: Duration = Duration::from_secs(10);

/// The level, from the share of the budget in use over time: up at once, down a step at a time.
#[derive(Debug)]
pub struct Governor {
    level: Pressure,
    since: Instant,
}

impl Governor {
    pub fn new(now: Instant) -> Governor {
        Governor { level: Pressure::Calm, since: now }
    }

    /// The level now that `used` (0.0 to 1.0 and over) of the budget is in use at `now`.
    pub fn update(&mut self, used: f32, now: Instant) -> Pressure {
        if !used.is_finite() {
            // (no reading: the level stays)
            return self.level;
        }
        let wanted = match self.level {
            // (the level it would be at from scratch)
            _ if used >= CRITICAL_UP => Pressure::Critical,
            Pressure::Critical if used >= CRITICAL_DOWN => Pressure::Critical,
            _ if used >= TIGHT_UP => Pressure::Tight,
            Pressure::Tight | Pressure::Critical if used >= TIGHT_DOWN => Pressure::Tight,
            _ => Pressure::Calm,
        };
        if wanted > self.level {
            self.level = wanted;
            self.since = now;
        } else if wanted < self.level && now.duration_since(self.since) >= HOLD {
            self.level = match self.level {
                Pressure::Critical => Pressure::Tight.max(wanted),
                _ => wanted,
            };
            self.since = now;
        }
        self.level
    }
}

/// The adapter the device was opened on, kept to ask its memory (see `Renderer::new`).
pub(crate) static ADAPTER: Mutex<Option<wgpu::Adapter>> = Mutex::new(None);

struct Live {
    governor: Governor,
    asked: Instant,
    logged: Instant,
    level: Pressure,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

/// The graphics memory's level now. The driver is asked once a second at most.
pub fn memory_pressure() -> Pressure {
    if !omsi_cfg::flags::OMSI_MEMORY_PRESSURE.is_set() {
        return Pressure::Calm;
    }
    let now = Instant::now();
    let mut guard = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    let live = guard.get_or_insert_with(|| Live { governor: Governor::new(now), asked: now - Duration::from_secs(60), logged: now, level: Pressure::Calm });
    if now.duration_since(live.asked) < Duration::from_secs(1) {
        return live.level;
    }
    live.asked = now;
    let info = ADAPTER.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|a| a.memory_info());
    // (a backend that does not tell, or a budget too small to be one: nothing to go by)
    let Some((used, budget)) = info.and_then(|m| Some((m.usage_bytes?, m.budget_bytes?))).filter(|&(_, b)| b >= 512 << 20) else {
        live.level = Pressure::Calm;
        return live.level;
    };
    let before = live.level;
    live.level = live.governor.update(used as f32 / budget as f32, now);
    if live.level != before || now.duration_since(live.logged) >= Duration::from_secs(60) {
        live.logged = now;
        log::info!("graphics memory: {} of {} MB in use ({:.0} %), {:?}", used >> 20, budget >> 20, 100.0 * used as f32 / budget as f32, live.level);
    }
    live.level
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, base: Instant) -> Instant {
        base + Duration::from_secs(secs)
    }

    #[test]
    fn it_rises_at_once_and_falls_one_step_at_a_time_after_a_hold() {
        let t0 = Instant::now();
        let mut g = Governor::new(t0);
        assert_eq!(g.update(0.50, at(0, t0)), Pressure::Calm);
        assert_eq!(g.update(0.86, at(1, t0)), Pressure::Tight);
        assert_eq!(g.update(0.96, at(2, t0)), Pressure::Critical);
        // memory is free again, but the level holds for ten seconds and then falls one step
        assert_eq!(g.update(0.30, at(5, t0)), Pressure::Critical, "3 s after the rise");
        assert_eq!(g.update(0.30, at(11, t0)), Pressure::Critical, "9 s after the rise");
        assert_eq!(g.update(0.30, at(12, t0)), Pressure::Tight, "10 s after the rise: one step");
        assert_eq!(g.update(0.30, at(20, t0)), Pressure::Tight, "8 s at the new level");
        assert_eq!(g.update(0.30, at(22, t0)), Pressure::Calm, "10 s at the new level");
        // a new rise in the middle of the way down is at once, and the way down starts again
        assert_eq!(g.update(0.90, at(23, t0)), Pressure::Tight);
        assert_eq!(g.update(0.30, at(30, t0)), Pressure::Tight);
        assert_eq!(g.update(0.30, at(33, t0)), Pressure::Calm);
    }

    #[test]
    fn the_edges_do_not_flicker() {
        let t0 = Instant::now();
        let mut g = Governor::new(t0);
        // a reading that wobbles round the edge of Tight stays Tight while it is above the way down
        assert_eq!(g.update(0.86, at(0, t0)), Pressure::Tight);
        for (i, u) in [0.84, 0.86, 0.80, 0.85, 0.78, 0.84].iter().enumerate() {
            assert_eq!(g.update(*u, at(60 + i as u64, t0)), Pressure::Tight, "{u}");
        }
        assert_eq!(g.update(0.70, at(80, t0)), Pressure::Calm);
        // and round the edge of Critical
        assert_eq!(g.update(0.96, at(90, t0)), Pressure::Critical);
        for (i, u) in [0.94, 0.96, 0.90, 0.95, 0.89].iter().enumerate() {
            assert_eq!(g.update(*u, at(100 + i as u64 * 20, t0)), Pressure::Critical, "{u}");
        }
    }

    #[test]
    fn a_jump_from_calm_to_critical_and_a_budget_overrun_are_critical() {
        let t0 = Instant::now();
        let mut g = Governor::new(t0);
        assert_eq!(g.update(0.99, t0), Pressure::Critical);
        assert_eq!(g.update(1.5, at(1, t0)), Pressure::Critical);
        assert_eq!(g.update(f32::NAN, at(2, t0)), Pressure::Critical, "no reading is not a calm one: the level holds");
    }

    #[test]
    fn without_the_switch_the_memory_is_always_calm() {
        // (the test process does not set OMSI_MEMORY_PRESSURE)
        if !omsi_cfg::flags::OMSI_MEMORY_PRESSURE.is_set() {
            assert_eq!(memory_pressure(), Pressure::Calm);
        }
    }
}

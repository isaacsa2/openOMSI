//! The texture budget against the driver's own count of the graphics memory in use.
//!
//! On Windows a Direct3D 12 or Vulkan process that goes past what the driver lets it keep
//! in the card's memory gets no out-of-memory error: the system moves its allocations to the
//! computer's memory and the GPU reads them over the bus from then on. The frame rate falls
//! by two thirds and more and stays there, with the time spent waiting for the picture to be
//! presented (#2121), while the game's own reaction to running out (`take_out_of_memory`)
//! never comes. The texture budget is set once from what the card is taken to hold
//! (`texture_budget`, `texture_overdraft`); the driver's count is what tells when it was
//! too much for this computer - the picture's targets, other programs, an APU's share.
//!
//! So while the memory in use stays near the driver's budget, the textures give way a fifth
//! at a time, before the card pages. They never get the room back during the session: a
//! budget that went up and down again was a stutter at each turn (see the render scale's
//! governor, #2295).

use std::time::{Duration, Instant};

/// The share of the driver's budget (percent) past which the textures give way.
const HIGH: u64 = 95;
/// Looks a second apart in a row that must find the memory so: a burst of uploads while
/// tiles stream in passes.
const STREAK: u32 = 3;
/// After a cut the far textures need time to lose their levels before the count shows it.
const SETTLE: Duration = Duration::from_secs(10);
/// Looks closer together than this count as one.
const SPACING: Duration = Duration::from_secs(1);
/// The budget is never cut below this (the floor of the out-of-memory path as well).
const FLOOR: u64 = 300_000_000;

/// What the last looks at the driver's count found.
#[derive(Default)]
pub(crate) struct MemoryWatch {
    streak: u32,
    looked: Option<Instant>,
    cut: Option<Instant>,
}

impl MemoryWatch {
    /// The texture budget to keep from now on when `used` bytes of the `allowed` the driver
    /// lets the process keep have stayed above `HIGH` percent for `STREAK` looks: `limit`
    /// a fifth smaller, not below `FLOOR`. None while there is room, or nothing left to cut.
    pub(crate) fn look(
        &mut self,
        limit: u64,
        used: u64,
        allowed: u64,
        now: Instant,
    ) -> Option<u64> {
        if self
            .looked
            .is_some_and(|t| now.saturating_duration_since(t) < SPACING)
        {
            return None;
        }
        self.looked = Some(now);
        if self
            .cut
            .is_some_and(|t| now.saturating_duration_since(t) < SETTLE)
        {
            return None;
        }
        if allowed == 0 || used.saturating_mul(100) < allowed.saturating_mul(HIGH) {
            self.streak = 0;
            return None;
        }
        self.streak += 1;
        if self.streak < STREAK {
            return None;
        }
        self.streak = 0;
        let less = (limit / 5 * 4).max(FLOOR);
        if less >= limit {
            return None;
        }
        self.cut = Some(now);
        Some(less)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    /// Looks a second apart from `t0`, each with `used` of `allowed`.
    fn looks(
        w: &mut MemoryWatch,
        t0: Instant,
        from_s: u64,
        n: u64,
        limit: u64,
        used: u64,
        allowed: u64,
    ) -> Vec<Option<u64>> {
        (from_s..from_s + n)
            .map(|s| w.look(limit, used, allowed, t0 + Duration::from_secs(s)))
            .collect()
    }

    #[test]
    fn room_to_spare_keeps_the_budget() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        assert!(looks(&mut w, t0, 0, 20, 1000 * MB, 900 * MB, 1000 * MB)
            .iter()
            .all(Option::is_none));
    }

    #[test]
    fn a_full_card_cuts_a_fifth_after_three_looks() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        let r = looks(&mut w, t0, 0, 3, 1000 * MB, 980 * MB, 1000 * MB);
        assert_eq!(r, vec![None, None, Some(800 * MB)]);
    }

    #[test]
    fn a_short_burst_is_not_a_full_card() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        assert_eq!(
            looks(&mut w, t0, 0, 2, 1000 * MB, 990 * MB, 1000 * MB),
            vec![None, None]
        );
        // (room again: the streak starts over)
        assert_eq!(
            w.look(1000 * MB, 500 * MB, 1000 * MB, t0 + Duration::from_secs(2)),
            None
        );
        assert_eq!(
            looks(&mut w, t0, 3, 2, 1000 * MB, 990 * MB, 1000 * MB),
            vec![None, None]
        );
    }

    #[test]
    fn looks_in_the_same_second_count_once() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        for ms in 0..50 {
            assert_eq!(
                w.look(
                    1000 * MB,
                    990 * MB,
                    1000 * MB,
                    t0 + Duration::from_millis(ms * 10)
                ),
                None
            );
        }
        assert_eq!(w.streak, 1);
    }

    #[test]
    fn a_cut_waits_for_the_textures_to_shrink() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        assert_eq!(
            looks(&mut w, t0, 0, 3, 1000 * MB, 990 * MB, 1000 * MB)[2],
            Some(800 * MB)
        );
        // still full right after the cut: nothing until it settled, then three looks again
        assert!(looks(&mut w, t0, 3, 9, 800 * MB, 990 * MB, 1000 * MB)
            .iter()
            .all(Option::is_none));
        assert_eq!(
            looks(&mut w, t0, 12, 3, 800 * MB, 990 * MB, 1000 * MB),
            vec![None, None, Some(640 * MB)]
        );
    }

    #[test]
    fn the_budget_stops_at_its_floor() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        assert_eq!(
            looks(&mut w, t0, 0, 3, 350 * MB, 990 * MB, 1000 * MB)[2],
            Some(FLOOR)
        );
        let mut w = MemoryWatch::default();
        assert!(looks(&mut w, t0, 0, 6, FLOOR, 990 * MB, 1000 * MB)
            .iter()
            .all(Option::is_none));
    }

    #[test]
    fn a_driver_without_a_budget_is_left_alone() {
        let mut w = MemoryWatch::default();
        let t0 = Instant::now();
        assert!(looks(&mut w, t0, 0, 6, 1000 * MB, 990 * MB, 0)
            .iter()
            .all(Option::is_none));
    }
}

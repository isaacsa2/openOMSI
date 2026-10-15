//! Temporary texture limits used when restarting after a lost graphics device.

/// Convert a live byte budget to the restart's decimal MB limit, rounding down.
/// Zero means unlimited and must not turn into a one-MB cap.
pub fn restart_limit_mb(bytes: u64) -> Option<u32> {
    (bytes > 0).then(|| (bytes / 1_000_000).clamp(1, u32::MAX as u64) as u32)
}

/// The safer profile may tighten a live limit, but must never raise it on restart.
pub fn texture_budget_mb(configured: u32, attempt: u32, previous: Option<u32>) -> u32 {
    let configured = if configured == 0 { 1200 } else { configured };
    let mut reduced = (u64::from(configured) * 2 / 3).max(400) as u32;
    if attempt >= 2 {
        reduced = reduced.min(700);
    }
    match previous.filter(|mb| *mb > 0) {
        Some(limit) => reduced.min(limit),
        None => reduced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reported_1440_mb_limit_survives_both_restarts() {
        let carried = restart_limit_mb(1_440_000_000);
        assert_eq!(texture_budget_mb(4000, 1, carried), 1440);
        assert_eq!(texture_budget_mb(4000, 2, carried), 700);
    }

    #[test]
    fn no_limit_or_a_larger_limit_keeps_the_existing_safe_profile() {
        for cap in [None, Some(0), Some(3500)] {
            assert_eq!(texture_budget_mb(4000, 1, cap), 2666);
            assert_eq!(texture_budget_mb(4000, 2, cap), 700);
        }
        assert_eq!(texture_budget_mb(0, 1, Some(2400)), 800);
        assert_eq!(texture_budget_mb(300, 1, Some(180)), 180);
        assert_eq!(texture_budget_mb(u32::MAX, 1, None), (u64::from(u32::MAX) * 2 / 3) as u32);
    }

    #[test]
    fn byte_limits_round_down_and_unlimited_stays_unset() {
        assert_eq!(restart_limit_mb(0), None);
        assert_eq!(restart_limit_mb(1), Some(1));
        assert_eq!(restart_limit_mb(1_440_999_999), Some(1440));
        assert_eq!(restart_limit_mb(u64::MAX), Some(u32::MAX));
    }
}

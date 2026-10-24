//! OMSI_PROFILE's account of `WindowWipers::update` (the frame's `player.glass`): its time
//! by part, logged every ten seconds of play.

/// The parts of `WindowWipers::update` measured.
const PARTS: [&str; 7] = ["water", "drops", "wipe", "mask", "mask upload", "renderer", "paint"];

/// Milliseconds by part (`PARTS`) and the frames they took since the last line in the log.
#[derive(Default)]
pub(super) struct GlassTimes {
    ms: [f64; 7],
    frames: u32,
    since: f64,
}

impl GlassTimes {
    pub(super) fn new(now: f64) -> Self {
        GlassTimes { since: now, ..Default::default() }
    }

    /// One frame's `clock`; every ten seconds of play (`now`) the milliseconds a frame by part,
    /// with the panes, those wiped apart, the blades and the drops on the glass (`what`).
    pub(super) fn add(&mut self, clock: &GlassLap, now: f64, what: (usize, usize, usize, usize)) {
        for (sum, ms) in self.ms.iter_mut().zip(clock.ms) {
            *sum += ms;
        }
        self.frames += 1;
        if now - self.since < 10.0 {
            return;
        }
        let per_frame = |ms: f64| ms / self.frames.max(1) as f64;
        let parts: Vec<String> = PARTS.iter().zip(self.ms).map(|(name, ms)| format!("{name} {:.3}", per_frame(ms))).collect();
        let (panes, wiped_apart, blades, drops) = what;
        log::info!(
            "window glass: {panes} panes ({wiped_apart} wiped apart), {blades} blades, {drops} drops; ms a frame over {} frames: {} (all {:.3})",
            self.frames,
            parts.join(", "),
            per_frame(self.ms.iter().sum())
        );
        *self = GlassTimes::new(now);
    }
}

/// One `update` timed part by part; nothing is read of the clock without OMSI_PROFILE.
pub(super) struct GlassLap {
    last: Option<std::time::Instant>,
    ms: [f64; 7],
}

impl GlassLap {
    pub(super) fn start() -> Self {
        GlassLap { last: omsi_cfg::flags::OMSI_PROFILE.is_set().then(std::time::Instant::now), ms: [0.0; 7] }
    }

    pub(super) fn timed(&self) -> bool {
        self.last.is_some()
    }

    /// The time since the last lap goes to part `k` (`PARTS`).
    pub(super) fn lap(&mut self, k: usize) {
        if let Some(last) = self.last.as_mut() {
            let now = std::time::Instant::now();
            self.ms[k] += (now - *last).as_secs_f64() * 1000.0;
            *last = now;
        }
    }
}

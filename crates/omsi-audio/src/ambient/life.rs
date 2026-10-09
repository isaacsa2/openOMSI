//! The living background: birds by day (recordings, played as an endless scatter of
//! excerpts), crickets on summer nights (synthesised, chirping at the rate the temperature
//! sets) and the far hum of the town's traffic.

use super::dsp::{db, flush, Biquad, Lag, Ou, Pink, Ramp, Rng};
use super::weather::smoothstep;
use super::AmbientParams;
use crate::mixer::Clip;
use std::sync::Arc;

/// A bell over the days of the year around `centre` (days), `width` its half width.
fn season_bell(day: f32, centre: f32, width: f32) -> f32 {
    let mut d = (day - centre).abs();
    if d > 182.5 {
        d = 365.0 - d;
    }
    (-(d / width).powi(2)).exp()
}

/// How much the birds sing now (0 … 1): the dawn chorus from civil twilight to a little
/// after sunrise, a quieter day, a short evening song, silence at night; most in the
/// breeding season (April to June), little in winter; less in rain, strong wind and hard
/// frost; more with trees around than on a bare street.
pub fn bird_activity(p: &AmbientParams) -> f32 {
    let el = p.sun_elevation;
    let dawn = (-((el - 2.0) / 5.0).powi(2)).exp() * if p.hour < 12.0 { 1.0 } else { 0.45 };
    let day = smoothstep(-2.0, 10.0, el) * 0.35;
    let diurnal = dawn.max(day);
    let season = 0.15 + 0.85 * season_bell(p.day_of_year, 135.0, 55.0) + 0.15 * season_bell(p.day_of_year, 270.0, 25.0);
    let rain = 1.0 - 0.85 * (p.rain_mm_h / 4.0).clamp(0.0, 1.0);
    let wind = 1.0 - 0.8 * smoothstep(7.0, 14.0, p.wind_10m);
    let cold = 0.3 + 0.7 * smoothstep(-12.0, 2.0, p.temperature);
    let habitat = 0.25 + 0.75 * p.foliage.clamp(0.0, 1.0);
    let snow = if p.snowing { 0.4 } else { 1.0 };
    (diurnal * season * rain * wind * cold * habitat * snow * p.sky_open.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// How many crickets sing (0 … 1): on summer nights, warmer than some 12 °C, dry and
/// calm, where there is green.
pub fn cricket_activity(p: &AmbientParams) -> f32 {
    let night = smoothstep(2.0, -4.0, p.sun_elevation);
    let season = season_bell(p.day_of_year, 210.0, 40.0);
    let warm = smoothstep(11.0, 17.0, p.temperature);
    let dry = 1.0 - (p.rain_mm_h / 0.5).clamp(0.0, 1.0);
    let calm = 1.0 - smoothstep(6.0, 10.0, p.wind_10m);
    let green = ((0.2 + p.foliage.clamp(0.0, 1.0)) * (1.0 - 0.6 * p.urban.clamp(0.0, 1.0))).clamp(0.0, 1.0);
    let snow = if p.snowing || p.snow_cover > 0.2 { 0.0 } else { 1.0 };
    night * season * warm * dry * calm * green * snow * p.sky_open.clamp(0.0, 1.0)
}

/// A cricket's chirps per minute at `t` °C (Dolbear's law, in Celsius).
pub fn chirps_per_minute(t: f32) -> f32 {
    (7.0 * t - 30.0).max(0.0)
}

/// How busy the roads of the town are at hour `h` (0 … 1): the usual weekday curve of
/// traffic counts, with its morning and afternoon peaks and the empty hours of the night.
pub fn traffic_profile(h: f32) -> f32 {
    let h = h.rem_euclid(24.0);
    let night = 0.08;
    let am = (-((h - 7.75) / 1.3).powi(2)).exp();
    let pm = 0.95 * (-((h - 17.0) / 1.8).powi(2)).exp();
    let day = 0.7 * smoothstep(5.5, 9.0, h) * (1.0 - smoothstep(19.0, 23.5, h));
    (night + am.max(pm).max(day)).min(1.0)
}

#[derive(Clone, Copy, Default)]
struct Cricket {
    freq: f32,
    phase: f32,
    gain: f32,
    pan: f32,
    /// Time to the next chirp (s), the time into the current chirp, its pulses.
    wait: f32,
    t: f32,
    pulses: u32,
}

/// A field-cricket chorus: each cricket a carrier of 4.2 - 5.1 kHz, a chirp three or four
/// pulses of 18 ms, 33 ms apart, the chirps at the temperature's rate.
pub struct Crickets {
    rng: Rng,
    all: [Cricket; 7],
    level: Lag,
    chirp_gap: f32,
}

impl Crickets {
    pub fn new(seed: u32) -> Crickets {
        let mut rng = Rng::new(seed);
        let all = std::array::from_fn(|_| Cricket {
            freq: 4200.0 + 900.0 * rng.uniform(),
            phase: 0.0,
            gain: 0.25 + 0.75 * rng.uniform().powi(2),
            pan: rng.uniform(),
            wait: rng.uniform(),
            t: 1.0,
            pulses: 3 + (rng.uniform() * 2.0) as u32,
        });
        Crickets { rng, all, level: Lag::default(), chirp_gap: 1.0 }
    }

    pub fn control(&mut self, p: &AmbientParams, dt: f32) {
        self.level.step(cricket_activity(p), 2.0, dt);
        let cpm = chirps_per_minute(p.temperature).max(20.0);
        self.chirp_gap = 60.0 / cpm;
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], n: usize, rate: f32) {
        let lv = self.level.v;
        if lv < 1.0e-4 {
            return;
        }
        let dt = 1.0 / rate;
        let tau = std::f32::consts::TAU;
        let [l, r] = env;
        // how many sing: the louder the chorus, the more join in
        let singing = ((lv * self.all.len() as f32).ceil() as usize).clamp(1, self.all.len());
        for (k, c) in self.all.iter_mut().take(singing).enumerate() {
            let g = c.gain * lv * db(-17.0);
            let w = c.freq * tau / rate;
            for i in 0..n {
                c.wait -= dt;
                if c.wait <= 0.0 {
                    c.wait += self.chirp_gap * (0.9 + 0.2 * self.rng.uniform());
                    c.t = 0.0;
                }
                let mut e = 0.0;
                if c.t < 0.033 * c.pulses as f32 {
                    let in_pulse = c.t % 0.033;
                    if in_pulse < 0.018 {
                        e = (std::f32::consts::PI * in_pulse / 0.018).sin();
                    }
                }
                c.t += dt;
                if e > 0.0 {
                    c.phase += w;
                    if c.phase > tau {
                        c.phase -= tau;
                    }
                    let s = (c.phase.sin() + 0.15 * (2.0 * c.phase).sin()) * e * g;
                    l[i] += s * (1.0 - 0.7 * c.pan);
                    r[i] += s * (0.3 + 0.7 * c.pan);
                }
            }
            let _ = k;
        }
    }
}

/// An excerpt of a recording being played.
#[derive(Clone)]
struct Excerpt {
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    /// Samples into the excerpt and its length (output frames).
    at: usize,
    len: usize,
    gains: (f32, f32),
}

/// An endless, never-repeating bed made of recordings: excerpts of a few seconds from
/// anywhere in them, each from another direction, overlapping by an equal-power crossfade -
/// no loop point to hear, no excerpt twice in the same place.
pub struct Scatter {
    rng: Rng,
    clips: Vec<Arc<Clip>>,
    playing: Vec<Excerpt>,
    fade: usize,
    level: Ramp,
    /// Excerpt lengths (s).
    min_len: f32,
    max_len: f32,
}

impl Scatter {
    pub fn new(seed: u32, min_len: f32, max_len: f32) -> Scatter {
        Scatter { rng: Rng::new(seed), clips: Vec::new(), playing: Vec::with_capacity(4), fade: 0, level: Ramp::new(0.0), min_len, max_len }
    }

    pub fn set_clips(&mut self, clips: Vec<Arc<Clip>>) {
        self.clips = clips.into_iter().filter(|c| c.frames() > 48_000).collect();
        self.playing.clear();
    }

    pub fn has_clips(&self) -> bool {
        !self.clips.is_empty()
    }

    fn start(&mut self, rate: f32) {
        if self.clips.is_empty() {
            return;
        }
        let k = ((self.rng.uniform() * self.clips.len() as f32) as usize).min(self.clips.len() - 1);
        let clip = self.clips[k].clone();
        let step = clip.sample_rate as f64 / rate as f64;
        let want = ((self.min_len + (self.max_len - self.min_len) * self.rng.uniform()) * rate) as usize;
        let avail = ((clip.frames() as f64 - 2.0) / step) as usize;
        let len = want.min(avail).max(self.fade * 2 + 1);
        let span = (clip.frames() as f64 - len as f64 * step - 2.0).max(0.0);
        let pos = self.rng.uniform() as f64 * span;
        let pan = self.rng.uniform();
        // equal-power pan, kept away from the hard sides (a bird is never in one ear only)
        let a = (0.15 + 0.7 * pan) * std::f32::consts::FRAC_PI_2;
        self.playing.push(Excerpt { clip, pos, step, at: 0, len, gains: (a.cos(), a.sin()) });
    }

    pub fn control(&mut self, level: f32, n: usize, rate: f32) {
        self.fade = (0.3 * rate) as usize;
        self.level.to(level.clamp(0.0, 2.0), n);
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], n: usize, rate: f32) {
        if self.clips.is_empty() {
            return;
        }
        if self.level.cur < 1.0e-4 && self.level.target() < 1.0e-4 {
            // (silent: no excerpt runs, the next starts fresh when it is wanted)
            self.playing.clear();
            for _ in 0..n {
                self.level.tick();
            }
            return;
        }
        if self.playing.is_empty() {
            self.start(rate);
        }
        let fade = self.fade.max(1);
        let [l, r] = env;
        for i in 0..n {
            let g = self.level.tick();
            // the next excerpt comes in as the last one starts to fade out
            if self.playing.last().is_some_and(|e| e.at + fade == e.len) {
                self.start(rate);
            }
            let (mut sl, mut sr) = (0.0, 0.0);
            for e in self.playing.iter_mut() {
                let x = (e.at as f32 / fade as f32).min(((e.len - e.at) as f32 / fade as f32).min(1.0)).min(1.0);
                // equal power: sin of the quarter turn
                let w = (x * std::f32::consts::FRAC_PI_2).sin();
                let ch = e.clip.channels as usize;
                let i0 = e.pos as usize;
                let t = (e.pos - i0 as f64) as f32;
                let at = |j: usize, c: usize| e.clip.samples[(j * ch + c.min(ch - 1)).min(e.clip.samples.len() - 1)] as f32 / 32768.0;
                let s0 = at(i0, 0) + (at(i0 + 1, 0) - at(i0, 0)) * t;
                let s1 = if ch > 1 { at(i0, 1) + (at(i0 + 1, 1) - at(i0, 1)) * t } else { s0 };
                // a stereo recording keeps its own image, narrowed toward its direction
                let (gl, gr) = e.gains;
                sl += (s0 * 0.7 + s1 * 0.3) * gl * w * 1.41;
                sr += (s1 * 0.7 + s0 * 0.3) * gr * w * 1.41;
                e.pos += e.step;
                e.at += 1;
            }
            self.playing.retain(|e| e.at < e.len);
            l[i] += sl * g;
            r[i] += sr * g;
        }
    }
}

/// The town far away: its traffic as a dull, slowly breathing roar, brown below some
/// 300 Hz with a little of the tyres' mid band; snow on the ground soaks up the highs.
pub struct City {
    rng: Rng,
    pink: [Pink; 2],
    brown: [f32; 2],
    low: [Biquad; 2],
    mid: [Biquad; 2],
    drift: Ou,
    level: Lag,
    mid_gain: f32,
}

impl City {
    pub fn new(seed: u32) -> City {
        City { rng: Rng::new(seed), pink: Default::default(), brown: [0.0; 2], low: Default::default(), mid: Default::default(), drift: Ou::default(), level: Lag::default(), mid_gain: 0.0 }
    }

    /// The hum's amplitude: traffic noise is a sum of many cars, its power goes with the
    /// number of them (+3 dB per doubling), the amplitude with its root. Around the ear the
    /// roads are as many as the place is built up (`urban`), and as near: sparser houses,
    /// roads farther apart - N sources at r ~ 1/sqrt(N) give N/r² ~ urban², so the town's
    /// own hum goes with `urban` itself (out in the fields it was barely below a town
    /// street's, louder than the birds there).
    pub fn amplitude(p: &AmbientParams) -> f32 {
        let urban = p.urban.clamp(0.0, 1.0);
        let town = urban * urban * traffic_profile(p.hour);
        town.max(p.traffic.clamp(0.0, 1.0)).sqrt()
    }

    pub fn control(&mut self, p: &AmbientParams, dt: f32, rate: f32) {
        let drift = self.drift.step(0.0, 1.5, 8.0, dt, &mut self.rng);
        self.level.step(Self::amplitude(p) * db(drift), 1.0, dt);
        let snow = p.snow_cover.clamp(0.0, 1.0);
        for c in 0..2 {
            self.low[c].lowpass(320.0 * (1.0 - 0.35 * snow), 0.7, rate);
            self.mid[c].bandpass(900.0 * (1.0 - 0.4 * snow), 0.6, rate);
        }
        self.mid_gain = 0.18 * (1.0 - 0.7 * snow);
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], n: usize) {
        let g = self.level.v * db(-17.0);
        if g < 1.0e-6 {
            return;
        }
        let [l, r] = env;
        for i in 0..n {
            for c in 0..2 {
                let w = self.rng.white();
                self.brown[c] = flush(self.brown[c] * 0.995 + w * 0.05);
                let y = (self.low[c].run(self.brown[c]) + self.mid[c].run(self.pink[c].next(w)) * self.mid_gain) * g;
                if c == 0 {
                    l[i] += y;
                } else {
                    r[i] += y;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(el: f32, day: f32, t: f32) -> AmbientParams {
        AmbientParams { sun_elevation: el, day_of_year: day, temperature: t, foliage: 0.8, sky_open: 1.0, hour: 6.0, ..Default::default() }
    }

    #[test]
    fn birds_sing_at_dawn_in_spring_not_at_night() {
        let dawn_may = bird_activity(&params(2.0, 135.0, 12.0));
        let night = bird_activity(&params(-20.0, 135.0, 12.0));
        let dawn_jan = bird_activity(&params(2.0, 15.0, 0.0));
        assert!(dawn_may > 0.6, "{dawn_may}");
        assert!(night < 0.01, "{night}");
        assert!(dawn_jan < dawn_may * 0.4, "{dawn_jan}");
    }

    #[test]
    fn crickets_need_a_warm_summer_night() {
        let summer_night = cricket_activity(&params(-15.0, 205.0, 20.0));
        let cold_night = cricket_activity(&params(-15.0, 205.0, 8.0));
        let summer_day = cricket_activity(&params(40.0, 205.0, 25.0));
        assert!(summer_night > 0.5, "{summer_night}");
        assert!(cold_night < 0.01 && summer_day < 0.01);
        // Dolbear: some 110 chirps a minute at 20 °C, none below about 4 °C
        assert!((chirps_per_minute(20.0) - 110.0).abs() < 1.0);
        assert_eq!(chirps_per_minute(3.0), 0.0);
    }

    #[test]
    fn rush_hours_are_busier_than_the_night() {
        assert!(traffic_profile(7.75) > 0.9);
        assert!(traffic_profile(3.0) < 0.15);
        assert!(traffic_profile(12.0) > 0.5);
    }

    #[test]
    fn scatter_crossfades_without_gaps_or_clicks() {
        // a constant-level clip: the excerpts overlap with an equal-power fade, so the sum
        // never dips to silence and never steps
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![8_000; 48_000 * 3] });
        let mut s = Scatter::new(9, 1.0, 1.5);
        s.set_clips(vec![clip]);
        let rate = 48_000.0;
        let n = 480;
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut all = Vec::new();
        for _ in 0..1000 {
            s.control(1.0, n, rate);
            l.iter_mut().for_each(|x| *x = 0.0);
            r.iter_mut().for_each(|x| *x = 0.0);
            s.render([&mut l, &mut r], n, rate);
            all.extend(l.iter().zip(r.iter()).map(|(a, b)| (a * a + b * b).sqrt()));
        }
        let settled = &all[48_000..];
        let lo = settled.iter().cloned().fold(f32::MAX, f32::min);
        let step = settled.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(lo > 0.05, "a gap: {lo}");
        assert!(step < 0.01, "a click: {step}");
    }
}

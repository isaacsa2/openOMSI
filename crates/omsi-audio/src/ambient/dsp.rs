//! The small building blocks of the ambience: noise, filters, smoothed parameters and a
//! random process for the wind. Everything runs on the audio thread, sample by sample, so
//! nothing here allocates and every state is flushed to zero before it can turn denormal
//! (a filter ringing out into the denormal range costs a hundred times a normal sample on
//! some CPUs - the classic CPU spike of a quiet reverb tail).

use std::f32::consts::PI;

/// Below this a state is set to zero (some 200 dB under full scale).
const TINY: f32 = 1.0e-10;

#[inline]
pub fn flush(x: f32) -> f32 {
    if x.abs() < TINY {
        0.0
    } else {
        x
    }
}

/// xorshift32: a fast, small random source (one per voice; never shared across threads).
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform in [-1, 1).
    #[inline]
    pub fn white(&mut self) -> f32 {
        self.uniform() * 2.0 - 1.0
    }

    /// Roughly normal (sum of four uniforms, unit variance).
    #[inline]
    pub fn gauss(&mut self) -> f32 {
        (self.uniform() + self.uniform() + self.uniform() + self.uniform() - 2.0) * 1.732_050_8
    }

    /// Exponentially distributed with mean 1.
    #[inline]
    pub fn exp1(&mut self) -> f32 {
        -(1.0 - self.uniform()).max(1.0e-7).ln()
    }
}

/// Pink noise (-3 dB per octave), Paul Kellett's economy filter over white noise: the
/// spectrum of most natural broadband sound (wind, distant traffic) falls off like this.
#[derive(Debug, Clone, Default)]
pub struct Pink {
    b: [f32; 3],
}

impl Pink {
    #[inline]
    pub fn next(&mut self, white: f32) -> f32 {
        self.b[0] = flush(0.99765 * self.b[0] + white * 0.099_046);
        self.b[1] = flush(0.963 * self.b[1] + white * 0.296_516_4);
        self.b[2] = flush(0.57 * self.b[2] + white * 1.052_691_3);
        (self.b[0] + self.b[1] + self.b[2] + white * 0.1848) * 0.11
    }
}

/// Brown (red) noise, -6 dB per octave: a leaky integrator of white noise.
#[derive(Debug, Clone, Default)]
pub struct Brown {
    y: f32,
}

impl Brown {
    #[inline]
    pub fn next(&mut self, white: f32) -> f32 {
        self.y = flush(self.y * 0.998 + white * 0.04);
        self.y
    }
}

/// One-pole low-pass (6 dB per octave).
#[derive(Debug, Clone, Default)]
pub struct OnePole {
    a: f32,
    z: f32,
}

impl OnePole {
    pub fn set(&mut self, hz: f32, rate: f32) {
        self.a = 1.0 - (-2.0 * PI * hz.max(1.0) / rate).exp();
    }

    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.z = flush(self.z + (x - self.z) * self.a);
        self.z
    }

    /// The matching high-pass (what the low-pass leaves out).
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        x - self.lp(x)
    }
}

/// A second-order section (RBJ cookbook), transposed direct form II.
#[derive(Debug, Clone, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn coeffs(&mut self, b: [f32; 3], a: [f32; 3]) {
        let inv = 1.0 / a[0];
        self.b0 = b[0] * inv;
        self.b1 = b[1] * inv;
        self.b2 = b[2] * inv;
        self.a1 = a[1] * inv;
        self.a2 = a[2] * inv;
    }

    fn w0(hz: f32, rate: f32) -> (f32, f32) {
        let w = 2.0 * PI * hz.clamp(5.0, rate * 0.45) / rate;
        (w.cos(), w.sin())
    }

    /// Band-pass with 0 dB at the centre (constant peak gain).
    pub fn bandpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, s) = Self::w0(hz, rate);
        let alpha = s / (2.0 * q.max(0.1));
        self.coeffs([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha]);
    }

    pub fn lowpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, s) = Self::w0(hz, rate);
        let alpha = s / (2.0 * q.max(0.1));
        let b = (1.0 - c) * 0.5;
        self.coeffs([b, 1.0 - c, b], [1.0 + alpha, -2.0 * c, 1.0 - alpha]);
    }

    pub fn highpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, s) = Self::w0(hz, rate);
        let alpha = s / (2.0 * q.max(0.1));
        let b = (1.0 + c) * 0.5;
        self.coeffs([b, -(1.0 + c), b], [1.0 + alpha, -2.0 * c, 1.0 - alpha]);
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = flush(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = flush(self.b2 * x - self.a2 * y);
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A parameter that moves to its new value over one block in a straight line instead of
/// jumping at the block's start: a gain stepped 60 times a second "zips" audibly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ramp {
    pub cur: f32,
    step: f32,
    left: u32,
}

impl Ramp {
    pub fn new(v: f32) -> Ramp {
        Ramp { cur: v, step: 0.0, left: 0 }
    }

    /// Head for `target`, arriving after `frames` samples.
    pub fn to(&mut self, target: f32, frames: usize) {
        let frames = frames.max(1) as u32;
        self.step = (target - self.cur) / frames as f32;
        self.left = frames;
    }

    /// Where the ramp is heading.
    pub fn target(&self) -> f32 {
        self.cur + self.step * self.left as f32
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.left > 0 {
            self.cur += self.step;
            self.left -= 1;
        }
        self.cur
    }
}

/// A value that follows its target with time constant `tau` (seconds), stepped once per
/// block: the crossfades between surfaces and weather states. Feed its value into a
/// [`Ramp`] for a sample-smooth curve.
#[derive(Debug, Clone, Copy, Default)]
pub struct Lag {
    pub v: f32,
}

impl Lag {
    pub fn step(&mut self, target: f32, tau: f32, dt: f32) -> f32 {
        let k = if tau <= 0.0 { 1.0 } else { 1.0 - (-dt / tau).exp() };
        self.v += (target - self.v) * k;
        if !self.v.is_finite() {
            self.v = 0.0;
        }
        self.v
    }
}

/// An Ornstein-Uhlenbeck process: a random quantity that wanders around `mean` with
/// standard deviation `sigma` and forgets its past over `tau` seconds - the textbook model
/// of the turbulent part of the wind (the gusts) around its mean speed.
#[derive(Debug, Clone, Default)]
pub struct Ou {
    pub x: f32,
}

impl Ou {
    pub fn step(&mut self, mean: f32, sigma: f32, tau: f32, dt: f32, rng: &mut Rng) -> f32 {
        let tau = tau.max(0.05);
        let a = (-dt / tau).exp();
        // exact discretisation: the stationary variance stays sigma² whatever dt is
        self.x = mean + (self.x - mean) * a + sigma * (1.0 - a * a).max(0.0).sqrt() * rng.gauss();
        if !self.x.is_finite() {
            self.x = mean;
        }
        self.x
    }
}

/// Decibels to a linear amplitude factor.
#[inline]
pub fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_centred() {
        let mut r = Rng::new(7);
        let mut p = Pink::default();
        let (mut sum, mut peak) = (0.0f64, 0.0f32);
        for _ in 0..200_000 {
            let x = p.next(r.white());
            sum += x as f64;
            peak = peak.max(x.abs());
        }
        assert!(peak < 1.0, "{peak}");
        assert!((sum / 200_000.0).abs() < 0.05, "{sum}");
    }

    #[test]
    fn filters_settle_to_exact_zero_without_input() {
        let mut b = Biquad::default();
        b.bandpass(1000.0, 8.0, 48_000.0);
        let mut lp = OnePole::default();
        lp.set(50.0, 48_000.0);
        let _ = b.run(1.0);
        let _ = lp.lp(1.0);
        let mut last = (1.0, 1.0);
        for _ in 0..2_000_000 {
            last = (b.run(0.0), lp.lp(0.0));
        }
        assert_eq!(last, (0.0, 0.0), "no denormal tail");
    }

    #[test]
    fn a_bandpass_passes_its_centre_at_unity() {
        let mut b = Biquad::default();
        b.bandpass(1000.0, 2.0, 48_000.0);
        let mut peak = 0.0f32;
        for k in 0..48_000 {
            let y = b.run((2.0 * PI * 1000.0 * k as f32 / 48_000.0).sin());
            if k > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - 1.0).abs() < 0.02, "{peak}");
    }

    #[test]
    fn the_wind_process_keeps_its_spread() {
        let mut r = Rng::new(3);
        let mut ou = Ou { x: 5.0 };
        let n = 200_000;
        let (mut s, mut s2) = (0.0f64, 0.0f64);
        for _ in 0..n {
            let x = ou.step(5.0, 1.5, 4.0, 0.01, &mut r) as f64;
            s += x;
            s2 += x * x;
        }
        let mean = s / n as f64;
        let sd = (s2 / n as f64 - mean * mean).sqrt();
        assert!((mean - 5.0).abs() < 0.3, "{mean}");
        assert!((sd - 1.5).abs() < 0.3, "{sd}");
    }

    #[test]
    fn a_ramp_arrives_in_a_straight_line() {
        let mut r = Ramp::new(0.0);
        r.to(1.0, 4);
        let v: Vec<f32> = (0..6).map(|_| r.tick()).collect();
        assert_eq!(v, vec![0.25, 0.5, 0.75, 1.0, 1.0, 1.0]);
    }
}

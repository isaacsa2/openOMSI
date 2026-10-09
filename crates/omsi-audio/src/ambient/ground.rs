//! The tyres on the road: what the surface under each wheel sounds like at the speed it
//! rolls at.
//!
//! - every surface: the tyre's own rolling noise, rising some 32 dB per decade of speed
//!   (the CPX measurements' B), its band moving up with the speed (tread blocks hitting the
//!   road at v / pitch);
//! - cobbles: one knock per stone, v / 0.16 m (a growl at town speeds), ringing the tyre's
//!   air cavity (~220 Hz) and the body's suspension thud;
//! - concrete slabs: a knock at every joint, v / 5.5 m (the "ta-dum" of the axles);
//! - gravel: thousands of grains crushed a second, and the odd stone flung against the
//!   wheel arch;
//! - dirt: a soft rumble, squelching when wet (mud);
//! - grass: a swish;
//! - snow: compacted crystals crunching, squeaking in hard frost;
//! - water: the hiss of the film on the road (deeper film, more spray), the swash of a
//!   puddle.

use super::dsp::{db, flush, Biquad, Lag, OnePole, Pink, Ramp, Rng};
use super::{Surface, WheelInput, SURFACES};

/// Speed at which the levels below hold (m/s, 50 km/h).
const V_REF: f32 = 14.0;

/// The level of a tyre heard at its reference distance (the wheel input's gain 1), against
/// the stock buses' sounds: a city bus's tyres at 40 km/h some 8 dB under its engine
/// heard from the pavement, on cobbles about as loud as the engine.
const LEVEL: f32 = 4.0;

/// What each surface knocks with: (spacing of the knocks along the road in m, relative
/// spread of the spacing, strength of a knock).
fn knocks(s: Surface) -> Option<(f32, f32, f32)> {
    match s {
        Surface::Cobble => Some((0.16, 0.35, 1.0)),
        Surface::Concrete => Some((5.5, 0.05, 0.6)),
        _ => None,
    }
}

/// How loud the tyre's own rolling noise is on each surface, relative to asphalt (dB).
fn rolling_db(s: Surface) -> f32 {
    match s {
        Surface::Asphalt => 0.0,
        Surface::Concrete => 2.0,
        Surface::Cobble => -2.0,
        Surface::Gravel => -4.0,
        Surface::Dirt => -9.0,
        Surface::Grass => -14.0,
        Surface::Snow => -10.0,
        Surface::DeepSnow => -16.0,
    }
}

pub struct Tyre {
    rng: Rng,
    /// The surfaces' weights, crossfading as the wheel rolls from one onto the next.
    weight: [Lag; SURFACES],
    speed: Lag,
    water: Lag,
    puddle: Lag,
    gain: [Ramp; 2],
    pink: Pink,
    roll: Biquad,
    roll_low: Biquad,
    /// Distance rolled since the last knock, and the distance to the next.
    rolled: f32,
    next_knock: f32,
    knock_env: f32,
    knock_decay: f32,
    cavity: Biquad,
    thud: Biquad,
    click: Biquad,
    grains: f32,
    grain_band: Biquad,
    ping: [Biquad; 2],
    ping_turn: usize,
    soft: OnePole,
    soft_band: Biquad,
    swish: Biquad,
    crunch: Biquad,
    squeal: Biquad,
    hiss: Biquad,
    hiss_hp: OnePole,
    wash: Biquad,
    mud_env: f32,
    mud: Biquad,
    floor: [OnePole; 2],
    through_body: bool,
    /// Per-block quantities.
    v: f32,
    temp: f32,
    load: f32,
    skip_base: bool,
    skip_wet: bool,
}

impl Tyre {
    pub fn new(seed: u32) -> Tyre {
        Tyre {
            rng: Rng::new(seed),
            weight: [Lag::default(); SURFACES],
            speed: Lag::default(),
            water: Lag::default(),
            puddle: Lag::default(),
            gain: [Ramp::default(); 2],
            pink: Pink::default(),
            roll: Biquad::default(),
            roll_low: Biquad::default(),
            rolled: 0.0,
            next_knock: 0.16,
            knock_env: 0.0,
            knock_decay: 0.0,
            cavity: Biquad::default(),
            thud: Biquad::default(),
            click: Biquad::default(),
            grains: 0.0,
            grain_band: Biquad::default(),
            ping: Default::default(),
            ping_turn: 0,
            soft: OnePole::default(),
            soft_band: Biquad::default(),
            swish: Biquad::default(),
            crunch: Biquad::default(),
            squeal: Biquad::default(),
            hiss: Biquad::default(),
            hiss_hp: OnePole::default(),
            wash: Biquad::default(),
            mud_env: 0.0,
            mud: Biquad::default(),
            floor: Default::default(),
            through_body: false,
            v: 0.0,
            temp: 10.0,
            load: 1.0,
            skip_base: false,
            skip_wet: false,
        }
    }

    /// The weight of surface `s` now (0 … 1, the crossfade's).
    pub fn weight(&self, s: Surface) -> f32 {
        self.weight[s as usize].v
    }

    /// Once per block (`n` frames, `dt` seconds).
    pub fn control(&mut self, w: &WheelInput, temperature: f32, n: usize, dt: f32, rate: f32) {
        // a tyre crosses a seam between two surfaces within a tenth of a second at
        // walking pace; faster than that the crossfade itself would click
        for (k, lag) in self.weight.iter_mut().enumerate() {
            lag.step(if w.surface as usize == k { 1.0 } else { 0.0 }, 0.08, dt);
        }
        self.v = self.speed.step(w.speed.abs(), 0.05, dt);
        self.water.step(w.water_mm.max(0.0), 0.3, dt);
        // a puddle is entered at once and left with its spray settling
        let tau = if w.puddle_mm > self.puddle.v { 0.01 } else { 0.12 };
        self.puddle.step(w.puddle_mm.max(0.0), tau, dt);
        self.temp = temperature;
        self.load = w.load.clamp(0.2, 2.5);
        self.skip_base = w.skip_base;
        self.skip_wet = w.skip_wet;
        self.through_body = w.through_body;
        self.gain[0].to(w.gain_l.max(0.0), n);
        self.gain[1].to(w.gain_r.max(0.0), n);
        let v = self.v;
        self.roll.bandpass(650.0 + 25.0 * v, 0.9, rate);
        self.roll_low.lowpass(160.0, 0.7, rate);
        self.knock_decay = (-1.0 / (0.0015 * rate)).exp();
        self.cavity.bandpass(220.0, 6.0, rate);
        self.thud.bandpass(70.0, 1.8, rate);
        self.click.highpass(1800.0, 0.7, rate);
        self.grain_band.bandpass(2600.0, 0.7, rate);
        self.soft.set(320.0, rate);
        self.soft_band.bandpass(520.0, 0.7, rate);
        self.swish.bandpass(1800.0, 0.5, rate);
        // dry snow squeaks the higher the colder it is (below some -5 °C the crystals
        // break instead of melting under pressure)
        let frost = (-5.0 - temperature).clamp(0.0, 15.0);
        self.crunch.bandpass(900.0 + 60.0 * frost, 1.2 + 0.25 * frost, rate);
        self.squeal.bandpass(1500.0 + 40.0 * frost, 8.0, rate);
        self.hiss.bandpass(4200.0, 0.6, rate);
        self.hiss_hp.set(1500.0, rate);
        self.wash.bandpass(650.0, 0.6, rate);
        self.mud.bandpass(180.0, 3.0, rate);
        for f in self.floor.iter_mut() {
            f.set(900.0, rate);
        }
    }

    /// Whether this wheel has anything to say this block.
    pub fn audible(&self) -> bool {
        (self.v > 0.2 || self.puddle.v > 0.5) && (self.gain[0].cur > 1.0e-5 || self.gain[1].cur > 1.0e-5 || self.gain[0].target() > 1.0e-5 || self.gain[1].target() > 1.0e-5)
    }

    /// Add `n` samples of this wheel into `out`.
    #[allow(clippy::needless_range_loop)]
    pub fn render(&mut self, out: [&mut [f32]; 2], n: usize, rate: f32) {
        if !self.audible() {
            // (the ramps still move, so a wheel coming back starts from where it was)
            for _ in 0..n {
                self.gain[0].tick();
                self.gain[1].tick();
            }
            return;
        }
        let v = self.v;
        let vr = v / V_REF;
        let w: [f32; SURFACES] = std::array::from_fn(|k| self.weight[k].v);
        // the rolling noise of the mix of surfaces under the tyre
        let base_skip = if self.skip_base { 0.0 } else { 1.0 };
        let mut roll_gain = 0.0;
        for (k, s) in Surface::ALL.iter().enumerate() {
            let own = if *s == Surface::Asphalt { base_skip } else { 1.0 };
            roll_gain += w[k] * db(rolling_db(*s)) * own;
        }
        let snow = w[Surface::Snow as usize] + w[Surface::DeepSnow as usize];
        roll_gain *= vr.powf(1.6) * self.load.powf(0.25) * db(-4.0);
        // knocks: the strongest knocking surface sets the spacing
        let knock = Surface::ALL
            .iter()
            .filter_map(|s| knocks(*s).map(|k| (w[*s as usize], k)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let (knock_w, (spacing, spread, strength)) = knock.unwrap_or((0.0, (1.0, 0.0, 0.0)));
        let knock_gain = knock_w * strength * vr.powf(0.9) * self.load.powf(0.5) * db(2.0);
        let gravel = w[Surface::Gravel as usize];
        let grain_rate = gravel * 480.0 * v + snow * 160.0 * v;
        let grain_p = (grain_rate / rate).min(0.6);
        let grain_gain = (gravel * db(-13.0) + snow * db(-7.0)) * vr.powf(0.5);
        let ping_p = gravel * 0.4 * v.powf(1.5) / rate;
        let dirt = w[Surface::Dirt as usize];
        let soft_gain = (dirt * db(-19.0) + w[Surface::Grass as usize] * db(-26.0)) * vr.powf(1.3);
        let swish_gain = w[Surface::Grass as usize] * db(-22.0) * vr.powf(1.2);
        // water: the film's hiss grows with the depth up to a millimetre or so and with the
        // spray (~ v²); a bus whose sound set has its own wet-road hiss keeps that one
        let film = (self.water.v / 1.0).min(1.5).sqrt();
        let hard = 1.0 - dirt - w[Surface::Grass as usize] - snow;
        let wet = if self.skip_wet { 0.0 } else { 1.0 };
        let hiss_gain = wet * film * hard.max(0.0) * vr * vr * db(-14.0);
        let mud = dirt * (self.water.v / 0.5).min(1.0);
        let mud_p = mud * 3.0 * v / rate;
        let wash_gain = (self.puddle.v / 20.0).min(2.0).sqrt() * (v / 6.0).min(2.5) * db(-4.0);
        let floor = self.through_body;
        let two = 1.0 / std::f32::consts::SQRT_2;
        for i in 0..n {
            let white = self.rng.white();
            let pink = self.pink.next(white);
            let mut y = 0.0;
            if roll_gain > 1.0e-6 {
                y += (self.roll.run(pink) + 0.7 * self.roll_low.run(pink)) * roll_gain;
            }
            if knock_gain > 1.0e-6 {
                self.rolled += v / rate;
                if self.rolled >= self.next_knock {
                    self.rolled -= self.next_knock;
                    self.next_knock = spacing * (1.0 + spread * (self.rng.uniform() * 2.0 - 1.0));
                    self.knock_env += 0.5 + 0.7 * self.rng.uniform();
                }
                // the rubber rolling over a stone's edge: a push of a millisecond or two
                let exc = self.knock_env * (0.6 + 0.4 * white);
                self.knock_env = flush(self.knock_env * self.knock_decay);
                y += (self.cavity.run(exc) * 0.9 + self.thud.run(exc) * 1.4 + self.click.run(exc * white) * 0.3) * knock_gain;
            }
            if grain_gain > 1.0e-6 {
                if self.rng.uniform() < grain_p {
                    self.grains = (self.grains + self.rng.exp1() * 0.6).min(4.0);
                }
                self.grains = flush(self.grains * 0.94);
                let g = white * self.grains;
                y += (self.grain_band.run(g) * gravel + self.crunch.run(g) * snow + self.squeal.run(g) * snow * ((-6.0 - self.temp) / 10.0).clamp(0.0, 1.0)) * grain_gain;
                let mut ping = 0.0;
                if self.rng.uniform() < ping_p {
                    // a stone against the wheel arch: a new pitch each time, on the other
                    // of two resonators (a ringing one retuned would click)
                    self.ping_turn ^= 1;
                    let f = 2500.0 + 2000.0 * self.rng.uniform();
                    self.ping[self.ping_turn].bandpass(f, 30.0, rate);
                    ping = 0.4 + 0.6 * self.rng.uniform();
                }
                y += (self.ping[0].run(if self.ping_turn == 0 { ping } else { 0.0 }) + self.ping[1].run(if self.ping_turn == 1 { ping } else { 0.0 })) * gravel * db(-16.0);
            }
            if soft_gain > 1.0e-6 {
                let b = self.soft.lp(white);
                y += (b * 3.0 + self.soft_band.run(pink)) * soft_gain;
            }
            if swish_gain > 1.0e-6 {
                y += self.swish.run(pink) * swish_gain;
            }
            if hiss_gain > 1.0e-6 {
                y += self.hiss.run(self.hiss_hp.hp(white)) * hiss_gain;
            }
            if mud > 1.0e-3 && v > 0.3 {
                // suction letting go of the tyre: low pops
                if self.rng.uniform() < mud_p {
                    self.mud_env = 1.0;
                }
                self.mud_env = flush(self.mud_env * 0.9993);
                y += self.mud.run(white * self.mud_env) * mud * db(-6.0);
            }
            if wash_gain > 1.0e-6 {
                y += self.wash.run(pink) * wash_gain;
            }
            let (gl, gr) = (self.gain[0].tick(), self.gain[1].tick());
            // heard through the floor: the panels pass the rumble, not the edge
            let y = if floor {
                let a = self.floor[1].lp(y);
                self.floor[0].lp(a)
            } else {
                y
            };
            out[0][i] += y * gl * two * LEVEL;
            out[1][i] += y * gr * two * LEVEL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(surface: Surface, speed: f32) -> WheelInput {
        WheelInput { speed, surface, gain_l: 1.0, gain_r: 1.0, load: 1.0, ..Default::default() }
    }

    /// RMS of a second of one wheel at `speed` on `surface`, after it settled.
    fn rms(surface: Surface, speed: f32) -> f32 {
        let rate = 48_000.0;
        let mut t = Tyre::new(11);
        let n = 480;
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut sum = 0.0f64;
        let mut count = 0usize;
        for block in 0..300 {
            t.control(&wheel(surface, speed), 5.0, n, n as f32 / rate, rate);
            l.iter_mut().for_each(|x| *x = 0.0);
            r.iter_mut().for_each(|x| *x = 0.0);
            t.render([&mut l, &mut r], n, rate);
            for x in &l {
                assert!(x.is_finite() && x.abs() < 4.0, "{surface:?} {x}");
            }
            if block >= 100 {
                sum += l.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
                count += n;
            }
        }
        (sum / count as f64).sqrt() as f32
    }

    #[test]
    fn rolling_noise_rises_with_speed() {
        let slow = rms(Surface::Asphalt, 5.0);
        let fast = rms(Surface::Asphalt, 20.0);
        // 32 dB a decade: 4x the speed is some 19 dB
        let gain_db = 20.0 * (fast / slow).log10();
        assert!(gain_db > 13.0 && gain_db < 25.0, "{gain_db}");
        assert!(rms(Surface::Asphalt, 0.0) < 1.0e-6, "standing still is silent");
    }

    #[test]
    fn cobbles_are_louder_than_asphalt_and_grass_quieter() {
        let asphalt = rms(Surface::Asphalt, 10.0);
        let cobble = rms(Surface::Cobble, 10.0);
        let grass = rms(Surface::Grass, 10.0);
        assert!(cobble > asphalt * 1.5, "{asphalt} {cobble}");
        assert!(grass < asphalt, "{asphalt} {grass}");
        for s in Surface::ALL {
            let x = rms(s, 15.0);
            assert!(x > 1.0e-4 && x < 0.5, "{s:?} {x}");
        }
    }

    #[test]
    fn a_surface_change_crossfades_without_a_jump() {
        let rate = 48_000.0;
        let mut t = Tyre::new(5);
        let n = 256;
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut weights = Vec::new();
        for block in 0..200 {
            let s = if block < 100 { Surface::Asphalt } else { Surface::Gravel };
            t.control(&wheel(s, 10.0), 5.0, n, n as f32 / rate, rate);
            weights.push(t.weight(Surface::Gravel));
            t.render([&mut l, &mut r], n, rate);
        }
        // the weight moves in small steps (each block a few percent), from 0 to 1
        assert!(weights[99] < 1.0e-3 && weights[199] > 0.95);
        let steepest = weights.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
        assert!(steepest < 0.08, "{steepest}");
    }
}

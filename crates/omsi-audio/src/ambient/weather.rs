//! The weather heard: the wind (its gusts, the whistle of overhead wires, the air rushing
//! past a moving bus), the leaves it shakes, the near raindrops and the thunder.
//!
//! Every level comes from a physical quantity: the noise of turbulent air grows with the
//! sixth power of its speed (a dipole source, Curle's law - so its pressure with the cube),
//! the rain from the drop-size spectrum of the rain rate (Marshall-Palmer) and the fall
//! speed of each drop, a wire whistles at the vortex-shedding frequency 0.2 u / d.

use super::dsp::{db, flush, Biquad, Lag, OnePole, Ou, Pink, Rng};
use super::AmbientParams;

/// Strouhal number of a cylinder in cross flow (vortex shedding).
const STROUHAL: f32 = 0.2;
/// Diameters of the overhead lines that sing in a storm (m): contact wires, cables.
const WIRES: [f32; 2] = [0.008, 0.013];
/// Speed at which the noise levels below are calibrated (m/s).
const U_REF: f32 = 10.0;

/// The wind at the listener's height (2 m) from the weather's 10 m wind over ground of
/// roughness length `z0` (m): the logarithmic profile of the surface layer. A city street
/// (z0 about 1 m) keeps a third of the wind, an open field (0.03 m) three quarters.
pub fn street_wind(u10: f32, z0: f32) -> f32 {
    let z0 = z0.clamp(0.01, 1.8);
    let ratio = ((2.0 / z0).ln() / (10.0 / z0).ln()).clamp(0.15, 0.9);
    u10.max(0.0) * ratio
}

/// Turbulence intensity (gust spread / mean) over ground of roughness `z0`: 1 / ln(z / z0)
/// at 10 m, about 0.17 over open land and 0.33 over a town.
pub fn turbulence_intensity(z0: f32) -> f32 {
    (1.0 / (10.0 / z0.clamp(0.01, 1.8)).ln()).clamp(0.1, 0.5)
}

pub struct Wind {
    rng: Rng,
    /// The gusty street-level speed, and the canopy's (both from one gust process).
    gust: Ou,
    pub u: f32,
    pub u_canopy: f32,
    pink: [Pink; 2],
    body: [Biquad; 2],
    hiss: [Biquad; 2],
    wires: [[Biquad; 2]; 2],
    flow: Biquad,
    flow_pink: Pink,
    level: Lag,
    whistle: Lag,
    flow_level: Lag,
}

impl Wind {
    pub fn new(seed: u32) -> Wind {
        Wind {
            rng: Rng::new(seed),
            gust: Ou::default(),
            u: 0.0,
            u_canopy: 0.0,
            pink: Default::default(),
            body: Default::default(),
            hiss: Default::default(),
            wires: Default::default(),
            flow: Biquad::default(),
            flow_pink: Pink::default(),
            level: Lag::default(),
            whistle: Lag::default(),
            flow_level: Lag::default(),
        }
    }

    /// Once per block: the gust process and the filters for the wind speed now.
    pub fn control(&mut self, p: &AmbientParams, dt: f32, rate: f32) {
        let mean = street_wind(p.wind_10m, p.roughness);
        let sigma = turbulence_intensity(p.roughness) * mean;
        // a gust is an eddy of some 50 m drifting past at the mean speed
        let tau = (50.0 / mean.max(0.5)).clamp(1.5, 20.0);
        self.u = self.gust.step(mean, sigma, tau, dt, &mut self.rng).max(0.0);
        let gust_ratio = if mean > 0.05 { self.u / mean } else { 1.0 };
        self.u_canopy = p.wind_10m.max(0.0) * gust_ratio;
        // broadband turbulence: its spectrum moves up with the speed (eddies of a given
        // size pass faster); pressure ~ u^3
        let fc = 60.0 + 35.0 * self.u;
        for c in 0..2 {
            self.body[c].bandpass(fc, 0.55, rate);
            self.hiss[c].bandpass(fc * 2.8, 0.9, rate);
        }
        let sky = p.sky_open.clamp(0.0, 1.0);
        let target = (self.u / U_REF).powi(3) * sky;
        self.level.step(target, 0.05, dt);
        // aeolian tones of the overhead lines (vortex shedding, f = St u / d): in town only
        // (the wires are there), and only in a real wind - below some 6 m/s the shedding
        // is too weak and irregular to sing
        for (k, d) in WIRES.iter().enumerate() {
            let f = STROUHAL * self.u / d;
            for c in 0..2 {
                self.wires[k][c].bandpass(f * (1.0 + 0.004 * c as f32), 45.0, rate);
            }
        }
        let sing = smoothstep(6.0, 13.0, self.u) * p.urban.clamp(0.0, 1.0) * (self.u / U_REF).powi(3) * sky;
        self.whistle.step(sing, 0.3, dt);
        // the air rushing along a moving bus's body and through its door seals, heard
        // inside: the speed relative to the air, ~ u^2.75 (measured interior wind noise
        // rises some 55 dB per decade of speed)
        let rel = p.air_speed.max(0.0);
        self.flow.bandpass(400.0 + 18.0 * rel, 0.7, rate);
        let flow = if p.inside { (rel / 20.0).powf(2.75) } else { 0.0 };
        self.flow_level.step(flow, 0.1, dt);
    }

    /// Add `n` samples: the outdoor wind into `env`, the flow noise of the bus into `dir`.
    pub fn render(&mut self, env: [&mut [f32]; 2], dir: [&mut [f32]; 2], n: usize) {
        let (lv, wv, fv) = (self.level.v, self.whistle.v, self.flow_level.v);
        if lv < 1.0e-6 && wv < 1.0e-6 && fv < 1.0e-6 {
            return;
        }
        let (body_gain, hiss_gain, wire_gain, flow_gain) = (db(-10.0) * lv, db(-26.0) * lv, db(-6.0) * wv, db(-8.0) * fv);
        let [el, er] = env;
        let [dl, dr] = dir;
        for i in 0..n {
            for c in 0..2 {
                let w = self.rng.white();
                let p = self.pink[c].next(w);
                let mut y = self.body[c].run(p) * body_gain + self.hiss[c].run(w) * hiss_gain;
                if wire_gain > 1.0e-7 {
                    let x = self.rng.white();
                    y += (self.wires[0][c].run(x) + 0.6 * self.wires[1][c].run(x)) * wire_gain;
                }
                if c == 0 {
                    el[i] += y;
                } else {
                    er[i] += y;
                }
            }
            if flow_gain > 1.0e-7 {
                let x = self.flow_pink.next(self.rng.white());
                let y = self.flow.run(x) * flow_gain;
                dl[i] += y;
                dr[i] += y;
            }
        }
    }
}

/// Leaves rustling: every leaf that knocks against another is a click of a millisecond or
/// two; how many knock per second grows with the wind in the crowns (and with how many
/// trees stand near), so a breeze is a sparse patter and a gale a continuous roar. Dry
/// autumn leaves are stiffer: shorter, brighter, louder clicks.
pub struct Leaves {
    rng: Rng,
    env: [f32; 2],
    band: [Biquad; 2],
    low: [OnePole; 2],
    decay: f32,
    rate_lr: [f32; 2],
    amp: f32,
    /// A click's strength: the leaves strike each other the harder the faster they move.
    strike: f32,
}

impl Leaves {
    pub fn new(seed: u32) -> Leaves {
        Leaves { rng: Rng::new(seed), env: [0.0; 2], band: Default::default(), low: Default::default(), decay: 0.0, rate_lr: [0.0; 2], amp: 0.0, strike: 0.0 }
    }

    /// Clicks per second of the trees around the listener in the canopy wind `u`.
    pub fn click_rate(p: &AmbientParams, u: f32) -> f32 {
        let leaves = (p.foliage * p.leaf_amount).clamp(0.0, 1.5);
        // (a leaf starts to flutter at some half a metre a second)
        9600.0 * leaves * (u - 0.5).max(0.0) * p.sky_open.clamp(0.0, 1.0)
    }

    pub fn control(&mut self, p: &AmbientParams, u_canopy: f32, rate: f32) {
        let dry = p.leaf_dryness.clamp(0.0, 1.0);
        let total = Self::click_rate(p, u_canopy);
        let bal = p.foliage_balance.clamp(-1.0, 1.0);
        self.rate_lr = [total * 0.5 * (1.0 - 0.6 * bal), total * 0.5 * (1.0 + 0.6 * bal)];
        // a click rings for 2.5 ms (green) to 1 ms (dry)
        let tau = 0.004 - 0.0015 * dry;
        self.decay = (-1.0 / (tau * rate)).exp();
        for c in 0..2 {
            self.band[c].bandpass(2200.0 + 1200.0 * dry + 150.0 * c as f32, 0.8, rate);
            self.low[c].set(500.0, rate);
        }
        self.amp = db(-49.0) * (0.8 + 0.4 * dry);
        self.strike = (u_canopy.max(0.0) / 10.0).sqrt();
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], n: usize, rate: f32) {
        if self.rate_lr[0] + self.rate_lr[1] < 0.5 {
            return;
        }
        let [l, r] = env;
        for i in 0..n {
            for c in 0..2 {
                // Poisson clicks: at most one new click a sample (a rate past the sample
                // rate is a continuous rustle anyway)
                let pr = (self.rate_lr[c] / rate).min(0.9);
                if self.rng.uniform() < pr {
                    // (a single leaf strikes no harder than a few times the mean)
                    let a = self.rng.exp1().min(3.0) * self.strike;
                    self.env[c] = (self.env[c] + a).min(60.0);
                }
                self.env[c] = flush(self.env[c] * self.decay);
                let x = self.rng.white() * self.env[c];
                let y = self.band[c].run(self.low[c].hp(x)) * self.amp;
                if c == 0 {
                    l[i] += y;
                } else {
                    r[i] += y;
                }
            }
        }
    }
}

/// Drops of `diameter` mm or more hitting one square metre each second in rain of
/// `mm_h` mm/h: the Marshall-Palmer spectrum N(D) = 8000 e^(-ΛD) per m³ and mm, Λ = 4.1
/// R^-0.21, times the fall speed 3.78 D^0.67 m/s (Atlas & Ulbrich).
pub fn drop_flux(mm_h: f32, min_d: f32) -> f32 {
    if mm_h <= 0.0 {
        return 0.0;
    }
    let lambda = 4.1 * mm_h.powf(-0.21);
    let (mut sum, steps, top) = (0.0f32, 64, min_d + 12.0 / lambda);
    let h = (top - min_d) / steps as f32;
    for k in 0..=steps {
        let d = min_d + h * k as f32;
        let w = if k == 0 || k == steps { 0.5 } else { 1.0 };
        sum += w * 8000.0 * (-lambda * d).exp() * 3.78 * d.powf(0.67);
    }
    sum * h
}

/// Drops of `min_d` mm or more in one cubic metre of air (what a windscreen sweeps up as
/// the bus drives through the rain).
pub fn drop_density(mm_h: f32, min_d: f32) -> f32 {
    if mm_h <= 0.0 {
        return 0.0;
    }
    let lambda = 4.1 * mm_h.powf(-0.21);
    8000.0 / lambda * (-lambda * min_d).exp()
}

/// The smallest drop whose impact is heard on its own (mm).
const AUDIBLE_DROP: f32 = 0.9;

/// A small bubble ringing in a puddle after a drop (Minnaert resonance).
#[derive(Clone, Copy, Default)]
struct Bubble {
    phase: f32,
    freq: f32,
    chirp: f32,
    amp: f32,
    decay: f32,
    pan: f32,
}

/// The rain near the ear: the separate drops on the ground and in the puddles around a
/// listener in the street, on the glass beside a listener in the bus, and - for a bus whose
/// own sound set has no rain on its roof - the roof drumming. (The rain's broad hiss in the
/// street is OMSI's `rain_outside.wav`, and the roof of a stock bus its own `regen.wav`.)
pub struct Rain {
    rng: Rng,
    /// Drops per second: on the ground nearby, on the glass, on the roof.
    ground_rate: f32,
    /// Drops a second from the wet trees around after the rain (see `control`).
    drip_rate: f32,
    glass_rate: f32,
    roof_rate: f32,
    lambda: f32,
    puddle_share: f32,
    click: [Biquad; 2],
    glass: [Biquad; 2],
    roof: [Biquad; 2],
    roof_env: f32,
    roof_gain: f32,
    roof_lp: OnePole,
    bubbles: [Bubble; 12],
    two_pi_over_rate: f32,
}

impl Rain {
    pub fn new(seed: u32) -> Rain {
        Rain {
            rng: Rng::new(seed),
            ground_rate: 0.0,
            drip_rate: 0.0,
            glass_rate: 0.0,
            roof_rate: 0.0,
            lambda: 4.0,
            puddle_share: 0.0,
            click: Default::default(),
            glass: Default::default(),
            roof: Default::default(),
            roof_env: 0.0,
            roof_gain: 0.0,
            roof_lp: OnePole::default(),
            bubbles: [Bubble::default(); 12],
            two_pi_over_rate: 0.0,
        }
    }

    pub fn control(&mut self, p: &AmbientParams, rate: f32) {
        let r = if p.snowing { 0.0 } else { p.rain_mm_h.max(0.0) } * p.sky_open.clamp(0.0, 1.0);
        self.lambda = 4.1 * r.max(0.01).powf(-0.21);
        let flux = drop_flux(r, AUDIBLE_DROP);
        // the ground within a metre or two of the ear (farther drops merge into the hiss)
        self.ground_rate = if p.inside { 0.0 } else { flux * 3.0 };
        // a windscreen of some 4 m² sweeps the air at the bus's speed, the side glass by the
        // ear takes the falling drops (and the wind blows some against it)
        self.glass_rate = if p.inside && p.glass_rain { flux * 1.5 + drop_density(r, AUDIBLE_DROP) * 4.0 * p.bus_speed.abs() * 0.25 } else { 0.0 };
        self.roof_rate = if p.inside && p.roof_rain { drop_flux(r, 0.5) * 25.0 } else { 0.0 };
        self.puddle_share = (p.wetness.clamp(0.0, 1.0) - 0.4).max(0.0) * 0.5;
        // after the rain the crowns drip on for a long while: the water a canopy holds (a
        // millimetre or so) runs off its leaves as big drops of 4 - 5 mm. The wet ground
        // stands for what the leaves still hold; a full canopy around the ear lets some
        // half a dozen a second fall within earshot.
        self.drip_rate = if p.inside || r > 0.05 { 0.0 } else { 6.0 * p.foliage.clamp(0.0, 1.0) * ((p.wetness.clamp(0.0, 1.0) - 0.2) / 0.8).max(0.0) };
        self.click[0].highpass(1500.0, 0.7, rate);
        self.click[1].highpass(1700.0, 0.7, rate);
        // a pane's lowest plate modes and the hollow of the sheet-metal roof
        self.glass[0].bandpass(2300.0, 7.0, rate);
        self.glass[1].bandpass(3900.0, 9.0, rate);
        self.roof[0].bandpass(650.0, 1.6, rate);
        self.roof[1].bandpass(1600.0, 2.0, rate);
        self.roof_lp.set(4000.0, rate);
        // thousands of drops a second sum like noise: the level goes with the root of the
        // rate (the drum's per-drop strength is set so that 4 mm/h on a bus roof is a
        // steady patter, not a roar)
        self.roof_gain = db(-40.0);
        self.two_pi_over_rate = std::f32::consts::TAU / rate;
    }

    /// A drop's diameter (mm), drawn from the spectrum above the audible size.
    fn drop(&mut self) -> f32 {
        // (drops over some 5.5 mm break up as they fall)
        (AUDIBLE_DROP + self.rng.exp1() / self.lambda).min(5.5)
    }

    /// The pressure of a drop's impact relative to a 3 mm one: its kinetic energy goes with
    /// D³ v² ~ D^4.34, the pressure with the root of that.
    fn impact(d: f32) -> f32 {
        (d / 3.0).powf(2.17)
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], dir: [&mut [f32]; 2], n: usize, rate: f32) {
        if self.ground_rate + self.drip_rate + self.glass_rate + self.roof_rate < 0.1 && self.bubbles.iter().all(|b| b.amp < 1.0e-5) {
            return;
        }
        let [el, er] = env;
        let [dl, dr] = dir;
        let p_ground = (self.ground_rate / rate).min(0.5);
        let p_drip = (self.drip_rate / rate).min(0.5);
        let p_glass = (self.glass_rate / rate).min(0.5);
        let roof_on = self.roof_rate > 0.1;
        let p_roof = (self.roof_rate / rate).min(1.0);
        for i in 0..n {
            // drops on the ground: a sharp tick, or a bubble in a puddle
            let (mut gl, mut gr) = (0.0, 0.0);
            if self.rng.uniform() < p_ground {
                let d = self.drop();
                let a = Self::impact(d) * (0.3 + 0.7 * self.rng.uniform());
                if self.rng.uniform() < self.puddle_share {
                    self.ring(d, a);
                } else if self.rng.uniform() < 0.5 {
                    gl += a;
                } else {
                    gr += a;
                }
            }
            // a drip from the leaves: a big drop, on the ground or into a puddle
            if self.rng.uniform() < p_drip {
                let d = 4.0 + self.rng.uniform();
                let a = Self::impact(d) * (0.4 + 0.6 * self.rng.uniform());
                if self.rng.uniform() < self.puddle_share * 1.5 {
                    self.ring(d, a);
                } else if self.rng.uniform() < 0.5 {
                    gl += a;
                } else {
                    gr += a;
                }
            }
            el[i] += self.click[0].run(gl) * db(-22.0);
            er[i] += self.click[1].run(gr) * db(-22.0);
            let (mut bl, mut br) = (0.0, 0.0);
            for b in self.bubbles.iter_mut() {
                if b.amp < 1.0e-5 {
                    continue;
                }
                b.phase += b.freq * self.two_pi_over_rate;
                if b.phase > std::f32::consts::TAU {
                    b.phase -= std::f32::consts::TAU;
                }
                b.freq *= b.chirp;
                let s = b.phase.sin() * b.amp;
                b.amp = flush(b.amp * b.decay);
                bl += s * (1.0 - b.pan);
                br += s * b.pan;
            }
            el[i] += bl * db(-16.0);
            er[i] += br * db(-16.0);
            // drops on the glass by the ear: the pane rings
            let mut g = 0.0;
            if self.rng.uniform() < p_glass {
                let d = self.drop();
                g = Self::impact(d) * (0.4 + 0.6 * self.rng.uniform());
            }
            let pane = self.glass[0].run(g) + 0.6 * self.glass[1].run(g);
            // the roof: thousands of drops a second, a continuous drumming that still
            // flickers with the single big ones
            let mut roof = 0.0;
            if roof_on {
                if self.rng.uniform() < p_roof {
                    let d = self.drop();
                    self.roof_env = (self.roof_env + Self::impact(d)).min(400.0);
                }
                self.roof_env = flush(self.roof_env * 0.995);
                let x = self.rng.white() * self.roof_env;
                roof = self.roof_lp.lp(self.roof[0].run(x) + 0.5 * self.roof[1].run(x)) * self.roof_gain;
            }
            dl[i] += pane * db(-12.0) + roof;
            dr[i] += pane * db(-15.0) + roof;
        }
    }

    /// Start a bubble for a drop of `d` mm that fell into water: its radius is a fraction
    /// of the drop's, and it rings at the Minnaert frequency 3.26 m/s / r, rising as it
    /// shrinks.
    fn ring(&mut self, d: f32, a: f32) {
        let r_mm = (0.25 + 0.6 * self.rng.uniform()) * d;
        let freq = (3.26 / (r_mm * 1.0e-3)).clamp(700.0, 9000.0);
        let k = self
            .bubbles
            .iter()
            .enumerate()
            .min_by(|x, y| x.1.amp.total_cmp(&y.1.amp))
            .map(|x| x.0)
            .unwrap_or(0);
        let rate = std::f32::consts::TAU / self.two_pi_over_rate.max(1.0e-9);
        self.bubbles[k] = Bubble {
            phase: 0.0,
            freq,
            chirp: 1.0 + 6.0 / rate,
            amp: a,
            decay: (-1.0 / ((0.004 + 0.006 * self.rng.uniform()) * rate)).exp(),
            pan: self.rng.uniform(),
        };
    }
}

/// Thunder of a thunderstorm (convective clouds and heavy rain): a flash every minute or so
/// somewhere within ten kilometres. Its loudness falls with the distance, the air takes the
/// high frequencies first (a near strike cracks, a far one only rumbles), and the rumble
/// lasts the longer the farther away it is (the channel's ends lie at more different
/// distances).
pub struct Thunder {
    rng: Rng,
    t: f32,
    len: f32,
    amp: f32,
    pan: f32,
    crack: f32,
    next: f32,
    brown: [f32; 2],
    lp: [Biquad; 2],
    peaks: [f32; 5],
    active: bool,
}

impl Thunder {
    pub fn new(seed: u32) -> Thunder {
        Thunder { rng: Rng::new(seed), t: 0.0, len: 0.0, amp: 0.0, pan: 0.5, crack: 0.0, next: 25.0, brown: [0.0; 2], lp: Default::default(), peaks: [0.0; 5], active: false }
    }

    pub fn control(&mut self, p: &AmbientParams, dt: f32, rate: f32) {
        let storm = p.convective && !p.snowing && p.rain_mm_h > 4.0;
        if self.active {
            self.t += dt;
            if self.t > self.len {
                self.active = false;
            }
            return;
        }
        if !storm {
            self.next = self.next.max(10.0);
            return;
        }
        self.next -= dt;
        if self.next > 0.0 {
            return;
        }
        // a strike: equally likely anywhere on the disc, so the distance's density ~ d
        let d_km = (1.0 + 99.0 * self.rng.uniform()).sqrt();
        self.next = 20.0 + 70.0 * self.rng.uniform();
        self.t = 0.0;
        self.len = 2.0 + 1.0 * d_km + self.rng.uniform();
        self.amp = 1.0 / d_km;
        self.crack = ((3.0 - d_km) / 2.0).clamp(0.0, 1.0);
        self.pan = self.rng.uniform();
        for k in 0..self.peaks.len() {
            self.peaks[k] = self.rng.uniform() * 0.6;
        }
        let fc = 9000.0 / (1.0 + 1.6 * d_km);
        for c in 0..2 {
            self.lp[c].lowpass(fc.max(120.0), 0.7, rate);
        }
        self.active = true;
    }

    /// The loudness envelope at `t` of a rumble `len` long: a quick rise, then the decay,
    /// with a few later peaks (the branches of the channel).
    fn envelope(&self, t: f32) -> f32 {
        let x = t / self.len.max(0.1);
        let base = (x / 0.03).min(1.0) * (-3.5 * x).exp();
        let mut extra = 0.0;
        for (k, at) in self.peaks.iter().enumerate() {
            let w = 0.05 + 0.02 * k as f32;
            extra += 0.6 * (-((x - at) / w).powi(2)).exp();
        }
        base * (1.0 + extra)
    }

    pub fn render(&mut self, env: [&mut [f32]; 2], n: usize, rate: f32) {
        if !self.active {
            return;
        }
        let [l, r] = env;
        let dt = 1.0 / rate;
        let mut t = self.t;
        for i in 0..n {
            let e = self.envelope(t) * self.amp;
            t += dt;
            for c in 0..2 {
                let w = self.rng.white();
                self.brown[c] = flush(self.brown[c] * 0.995 + w * 0.1);
                let crack = if self.rng.uniform() < 0.02 * self.crack { w * 8.0 } else { 0.0 };
                let x = (self.brown[c] + crack * (1.0 - t / self.len).max(0.0)) * e;
                let y = self.lp[c].run(x) * db(-9.0);
                let g = if c == 0 { 1.0 - 0.5 * self.pan } else { 0.5 + 0.5 * self.pan };
                if c == 0 {
                    l[i] += y * g;
                } else {
                    r[i] += y * g;
                }
            }
        }
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wind_profile_of_town_and_country() {
        let town = street_wind(10.0, 1.0);
        let field = street_wind(10.0, 0.03);
        assert!(town > 2.0 && town < 4.0, "{town}");
        assert!(field > 7.0 && field < 8.5, "{field}");
        assert!(turbulence_intensity(1.0) > turbulence_intensity(0.03));
    }

    #[test]
    fn the_trees_drip_after_the_rain() {
        let mut rain = Rain::new(7);
        let wet = AmbientParams { enabled: true, rain_mm_h: 0.0, wetness: 0.9, foliage: 1.0, ..Default::default() };
        rain.control(&wet, 48000.0);
        assert!(rain.drip_rate > 3.0, "{}", rain.drip_rate);
        // dry, raining (the drops are the rain's then), out in the open, or inside: none
        for p in [
            AmbientParams { wetness: 0.1, ..wet.clone() },
            AmbientParams { rain_mm_h: 4.0, ..wet.clone() },
            AmbientParams { foliage: 0.0, ..wet.clone() },
            AmbientParams { inside: true, ..wet.clone() },
        ] {
            rain.control(&p, 48000.0);
            assert_eq!(rain.drip_rate, 0.0);
        }
    }

    #[test]
    fn more_rain_more_drops() {
        let light = drop_flux(1.0, AUDIBLE_DROP);
        let heavy = drop_flux(15.0, AUDIBLE_DROP);
        assert!(light > 10.0 && light < 1000.0, "{light}");
        assert!(heavy > 4.0 * light, "{light} {heavy}");
        assert_eq!(drop_flux(0.0, 1.0), 0.0);
        // all drops of 1 mm/h: some thousands per m² and s
        let all = drop_flux(1.0, 0.0);
        assert!(all > 1500.0 && all < 4000.0, "{all}");
    }

    #[test]
    fn leaves_are_quiet_in_calm_air_and_on_bare_trees() {
        let mut p = AmbientParams { foliage: 1.0, leaf_amount: 1.0, sky_open: 1.0, ..Default::default() };
        assert_eq!(Leaves::click_rate(&p, 0.4), 0.0);
        let breeze = Leaves::click_rate(&p, 3.0);
        let gale = Leaves::click_rate(&p, 15.0);
        assert!(gale > 4.0 * breeze && breeze > 0.0);
        p.leaf_amount = 0.0;
        assert_eq!(Leaves::click_rate(&p, 15.0), 0.0);
    }
}

//! The ambience: openOMSI's own layer of sound over what OMSI 2's sound configurations
//! play - the wind and its gusts, leaves, the near raindrops, thunder, birds and crickets,
//! the town's far hum, and the tyres on the surface under each wheel.
//!
//! It is driven by physical quantities the game hands over once a frame
//! ([`AmbientParams`]: the wind speed, the rain rate, the speed and the surface of each
//! wheel, the sun's elevation, how open the bus is …) and synthesised on the audio thread,
//! so it follows them continuously and repeats nothing. What is heard *outside* (the
//! weather, nature, the town) passes through the bus's bodywork when the listener sits in
//! it: quieter and duller, opening up as the doors open (`Snd_OutsideVol`).
//!
//! Where OMSI 2 already plays a sound for something, this layer leaves it out: the rain's
//! hiss in the street (`rain_outside.wav`), a bus's own rain on its roof, its wet-road hiss
//! and rolling noise (see [`WheelInput::skip_base`]).

pub mod dsp;
pub mod ground;
pub mod life;
pub mod weather;

use dsp::{Lag, OnePole, Ramp};
use std::sync::Arc;

/// What a tyre rolls on. The numbers are OMSI's `[surface]` ids of a texture's `.cfg`
/// (`Axle_SurfaceID_` in the scripts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Surface {
    #[default]
    Asphalt = 0,
    Concrete = 1,
    Cobble = 2,
    Dirt = 3,
    Grass = 4,
    Gravel = 5,
    Snow = 6,
    DeepSnow = 7,
}

pub const SURFACES: usize = 8;

impl Surface {
    pub const ALL: [Surface; SURFACES] =
        [Surface::Asphalt, Surface::Concrete, Surface::Cobble, Surface::Dirt, Surface::Grass, Surface::Gravel, Surface::Snow, Surface::DeepSnow];

    /// From OMSI's `[surface]` id; 8 (the fresh snow of the winter textures' "snowfall"
    /// sets) is snow, anything unknown asphalt.
    pub fn from_omsi(id: u8) -> Surface {
        match id {
            1 => Surface::Concrete,
            2 => Surface::Cobble,
            3 => Surface::Dirt,
            4 => Surface::Grass,
            5 => Surface::Gravel,
            6 | 8 => Surface::Snow,
            7 => Surface::DeepSnow,
            _ => Surface::Asphalt,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Surface::Asphalt => "asphalt",
            Surface::Concrete => "concrete",
            Surface::Cobble => "cobblestone",
            Surface::Dirt => "dirt",
            Surface::Grass => "grass",
            Surface::Gravel => "gravel",
            Surface::Snow => "snow",
            Surface::DeepSnow => "deep snow",
        }
    }
}

/// One rolling tyre (or the pair of an axle's side), as the listener hears it.
#[derive(Debug, Clone, Copy, Default)]
pub struct WheelInput {
    /// Rolling speed over the ground (m/s).
    pub speed: f32,
    pub surface: Surface,
    /// Depth of the water film on the road under it (mm) and of a puddle it rolls through.
    pub water_mm: f32,
    pub puddle_mm: f32,
    /// Load relative to the tyre's load at rest.
    pub load: f32,
    /// Gains towards the left and right ear (distance, direction, the bodywork between).
    pub gain_l: f32,
    pub gain_r: f32,
    /// Heard from inside the bus, through its floor.
    pub through_body: bool,
    /// The vehicle's own sound set has a rolling noise (no asphalt base here then), and a
    /// wet-road hiss.
    pub skip_base: bool,
    pub skip_wet: bool,
}

/// The tyres heard at once (the player's bus: each side of the front and of the rear).
pub const MAX_WHEELS: usize = 4;

/// Everything the ambience is made from, handed over by the game once a frame.
#[derive(Debug, Clone, Copy)]
pub struct AmbientParams {
    pub enabled: bool,
    /// The setting's volume (0 … 1), on top of the master volume.
    pub volume: f32,
    /// The weather's mean wind at 10 m (m/s).
    pub wind_10m: f32,
    /// Aerodynamic roughness length of the surroundings (m): 0.03 open land … 1+ town.
    pub roughness: f32,
    /// How fast the listener's bus moves through the air (m/s), and over the ground.
    pub air_speed: f32,
    pub bus_speed: f32,
    /// Trees near the listener (0 none … 1 a park), and where they stand (-1 left … 1 right).
    pub foliage: f32,
    pub foliage_balance: f32,
    /// Leaves on them (0 bare … 1 full) and how dry they are (autumn).
    pub leaf_amount: f32,
    pub leaf_dryness: f32,
    /// Rain rate (mm/h); `snowing` when the precipitation is snow.
    pub rain_mm_h: f32,
    pub snowing: bool,
    /// Snow lying (0 … 1) and the wetness of the roads (0 … 1).
    pub snow_cover: f32,
    pub wetness: f32,
    /// Air temperature (°C).
    pub temperature: f32,
    /// The sun's elevation (degrees), the hour and the day of the year.
    pub sun_elevation: f32,
    pub hour: f32,
    pub day_of_year: f32,
    /// How built-up the place is (0 … 1) and the traffic heard around (0 … 1).
    pub urban: f32,
    pub traffic: f32,
    /// Thunderstorm clouds.
    pub convective: bool,
    /// The listener sits in the bus; how open it is to the outside (`Snd_OutsideVol`).
    pub inside: bool,
    pub open: f32,
    /// How much sky is over the listener (1 the open street … 0 a tunnel).
    pub sky_open: f32,
    /// Play the rain on the roof (the bus's sound set has none) and the drops on the glass.
    pub roof_rain: bool,
    pub glass_rain: bool,
    pub wheels: [WheelInput; MAX_WHEELS],
}

impl Default for AmbientParams {
    fn default() -> Self {
        AmbientParams {
            enabled: false,
            volume: 1.0,
            wind_10m: 0.0,
            roughness: 0.5,
            air_speed: 0.0,
            bus_speed: 0.0,
            foliage: 0.0,
            foliage_balance: 0.0,
            leaf_amount: 1.0,
            leaf_dryness: 0.0,
            rain_mm_h: 0.0,
            snowing: false,
            snow_cover: 0.0,
            wetness: 0.0,
            temperature: 15.0,
            sun_elevation: 30.0,
            hour: 12.0,
            day_of_year: 150.0,
            urban: 0.5,
            traffic: 0.0,
            convective: false,
            inside: false,
            open: 0.0,
            sky_open: 1.0,
            roof_rain: false,
            glass_rain: true,
            wheels: [WheelInput::default(); MAX_WHEELS],
        }
    }
}

/// The parts of the ambience, for the level meters.
pub const PARTS: [&str; 9] = ["wind", "leaves", "rain", "thunder", "birds", "crickets", "city", "tyres", "total"];

/// The ambience's synthesiser (lives on the audio thread).
pub struct Ambient {
    rate: f32,
    params: AmbientParams,
    wind: weather::Wind,
    leaves: weather::Leaves,
    rain: weather::Rain,
    thunder: weather::Thunder,
    crickets: life::Crickets,
    birds: life::Scatter,
    city: life::City,
    tyres: Vec<ground::Tyre>,
    /// What the outside passes through: (gain, cutoff) of the bodywork now, filtered twice.
    wall_gain: Lag,
    wall_cut: Lag,
    wall: [[OnePole; 2]; 2],
    wall_ramp: Ramp,
    out_gain: Ramp,
    /// The layer's own peak limiter (its gain now): a gust in a storm over a gravel road
    /// can sum past full scale before the master limiter sees the rest of the mix.
    limit: f32,
    limit_release: f32,
    on: Lag,
    /// Scratch buffers (outside, inside, one part) - allocated once, never on the audio
    /// thread's way.
    env: [Vec<f32>; 2],
    dir: [Vec<f32>; 2],
    part: [Vec<f32>; 2],
    /// Mean square of each part over the last second (see `levels`).
    meters: [f32; PARTS.len()],
}

/// The highest the layer reaches on its own (about -4 dBFS).
const LIMIT: f32 = 0.6;

/// Frames per block the ambience is worked out in.
const BLOCK: usize = 512;

impl Ambient {
    pub fn new(rate: u32) -> Ambient {
        let buf = || vec![0.0f32; BLOCK];
        Ambient {
            rate: rate.max(8000) as f32,
            params: AmbientParams::default(),
            wind: weather::Wind::new(0x1234),
            leaves: weather::Leaves::new(0x2345),
            rain: weather::Rain::new(0x3456),
            thunder: weather::Thunder::new(0x4567),
            crickets: life::Crickets::new(0x5678),
            birds: life::Scatter::new(0x6789, 4.0, 9.0),
            city: life::City::new(0x789a),
            tyres: (0..MAX_WHEELS).map(|k| ground::Tyre::new(0x9000 + k as u32 * 77)).collect(),
            wall_gain: Lag { v: 1.0 },
            wall_cut: Lag { v: 18_000.0 },
            wall: Default::default(),
            wall_ramp: Ramp::new(1.0),
            out_gain: Ramp::new(0.0),
            limit: 1.0,
            limit_release: (-1.0 / (0.08 * rate.max(8000) as f32)).exp(),
            on: Lag::default(),
            env: [buf(), buf()],
            dir: [buf(), buf()],
            part: [buf(), buf()],
            meters: [0.0; PARTS.len()],
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    pub fn set_params(&mut self, p: AmbientParams) {
        self.params = p;
    }

    pub fn set_bird_clips(&mut self, clips: Vec<Arc<crate::mixer::Clip>>) {
        self.birds.set_clips(clips);
    }

    pub fn has_birds(&self) -> bool {
        self.birds.has_clips()
    }

    /// RMS level of each part of [`PARTS`] (linear, before the master volume).
    pub fn levels(&self) -> [f32; PARTS.len()] {
        self.meters.map(|m| m.sqrt())
    }

    /// Add the ambience to `out` (`ch` interleaved channels), scaled by `master`.
    pub fn render(&mut self, out: &mut [f32], ch: usize, master: f32) {
        let ch = ch.max(1);
        let frames = out.len() / ch;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            self.block(&mut out[done * ch..(done + n) * ch], ch, n, master);
            done += n;
        }
    }

    fn block(&mut self, out: &mut [f32], ch: usize, n: usize, master: f32) {
        let rate = self.rate;
        let dt = n as f32 / rate;
        let p = self.params;
        // (switched off: faded out over a quarter second, then nothing is worked out)
        let on = self.on.step(if p.enabled { 1.0 } else { 0.0 }, 0.08, dt);
        if on < 1.0e-4 && !p.enabled {
            self.out_gain = Ramp::new(0.0);
            return;
        }
        for b in self.env.iter_mut().chain(self.dir.iter_mut()) {
            b[..n].iter_mut().for_each(|x| *x = 0.0);
        }
        self.wind.control(&p, dt, rate);
        self.leaves.control(&p, self.wind.u_canopy, rate);
        self.rain.control(&p, rate);
        self.thunder.control(&p, dt, rate);
        self.crickets.control(&p, dt);
        self.birds.control(life::bird_activity(&p) * dsp::db(6.0), n, rate);
        self.city.control(&p, dt, rate);
        for (t, w) in self.tyres.iter_mut().zip(p.wheels.iter()) {
            t.control(w, p.temperature, n, dt, rate);
        }
        // each part into its own scratch first (for its meter), then onto the buses
        let mut meters = [0.0f32; PARTS.len()];
        macro_rules! part {
            ($k:expr, $to_env:expr, $body:expr) => {{
                self.part[0][..n].iter_mut().for_each(|x| *x = 0.0);
                self.part[1][..n].iter_mut().for_each(|x| *x = 0.0);
                $body;
                let mut e = 0.0f32;
                let bus = if $to_env { &mut self.env } else { &mut self.dir };
                for c in 0..2 {
                    for i in 0..n {
                        let x = self.part[c][i];
                        e += x * x;
                        bus[c][i] += x;
                    }
                }
                meters[$k] = e / (2 * n) as f32;
            }};
        }
        {
            self.part.iter_mut().for_each(|b| b[..n].iter_mut().for_each(|x| *x = 0.0));
            let [a, b] = &mut self.part;
            let (a, b) = (&mut a[..n], &mut b[..n]);
            let [dl, dr] = &mut self.dir;
            self.wind.render([a, b], [&mut dl[..n], &mut dr[..n]], n);
        }
        meters[0] = mean_square(&self.part, n);
        add_into(&mut self.env, &self.part, n);
        part!(1, true, {
            let [a, b] = &mut self.part;
            self.leaves.render([&mut a[..n], &mut b[..n]], n, rate)
        });
        {
            self.part.iter_mut().for_each(|b| b[..n].iter_mut().for_each(|x| *x = 0.0));
            let [a, b] = &mut self.part;
            let [dl, dr] = &mut self.dir;
            self.rain.render([&mut a[..n], &mut b[..n]], [&mut dl[..n], &mut dr[..n]], n, rate);
            meters[2] = mean_square(&self.part, n);
            add_into(&mut self.env, &self.part, n);
        }
        part!(3, true, {
            let [a, b] = &mut self.part;
            self.thunder.render([&mut a[..n], &mut b[..n]], n, rate)
        });
        part!(4, true, {
            let [a, b] = &mut self.part;
            self.birds.render([&mut a[..n], &mut b[..n]], n, rate)
        });
        part!(5, true, {
            let [a, b] = &mut self.part;
            self.crickets.render([&mut a[..n], &mut b[..n]], n, rate)
        });
        part!(6, true, {
            let [a, b] = &mut self.part;
            self.city.render([&mut a[..n], &mut b[..n]], n)
        });
        part!(7, false, {
            let [a, b] = &mut self.part;
            for t in self.tyres.iter_mut() {
                t.render([&mut a[..n], &mut b[..n]], n, rate);
            }
        });
        // the bodywork between the outside and a listener in the bus: closed, a tenth of
        // the sound and little above 500 Hz (the mass law: the loss rises 6 dB an octave);
        // doors and windows open, nearly all of it
        let open = p.open.clamp(0.0, 1.0);
        let (g, fc) = if p.inside { (0.12 + 0.68 * open, 500.0 + 9000.0 * open * open) } else { (1.0, 18_000.0) };
        let g = self.wall_gain.step(g, 0.2, dt);
        let fc = self.wall_cut.step(fc, 0.2, dt);
        for f in self.wall.iter_mut().flatten() {
            f.set(fc, rate);
        }
        self.wall_ramp.to(g, n);
        self.out_gain.to(on * p.volume.clamp(0.0, 1.0) * master.max(0.0), n);
        let mut total = 0.0f32;
        for i in 0..n {
            let w = self.wall_ramp.tick();
            let og = self.out_gain.tick();
            let [wl, wr] = &mut self.wall;
            let el = wl[0].lp(self.env[0][i]);
            let er = wr[0].lp(self.env[1][i]);
            let l = wl[1].lp(el) * w + self.dir[0][i];
            let r = wr[1].lp(er) * w + self.dir[1][i];
            total += l * l + r * r;
            let (l, r) = (l * og, r * og);
            let peak = l.abs().max(r.abs());
            let want = if peak * self.limit > LIMIT { LIMIT / peak } else { 1.0 };
            self.limit = if want < self.limit { want } else { want + (self.limit - want) * self.limit_release };
            let (l, r) = (l * self.limit, r * self.limit);
            if ch == 1 {
                out[i] += 0.5 * (l + r);
            } else {
                out[i * ch] += l;
                out[i * ch + 1] += r;
            }
        }
        meters[8] = total / (2 * n) as f32;
        // a one-second meter
        let k = (dt / 1.0).min(1.0);
        for (m, v) in self.meters.iter_mut().zip(meters) {
            *m += (v - *m) * k;
            if !m.is_finite() {
                *m = 0.0;
            }
        }
    }
}

fn mean_square(b: &[Vec<f32>; 2], n: usize) -> f32 {
    let e: f32 = b.iter().map(|c| c[..n].iter().map(|x| x * x).sum::<f32>()).sum();
    e / (2 * n.max(1)) as f32
}

fn add_into(to: &mut [Vec<f32>; 2], from: &[Vec<f32>; 2], n: usize) {
    for c in 0..2 {
        for i in 0..n {
            to[c][i] += from[c][i];
        }
    }
}

#[cfg(test)]
mod tests;

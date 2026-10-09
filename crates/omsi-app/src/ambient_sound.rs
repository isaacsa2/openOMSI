//! openOMSI's ambience (settings `ambient`, `vol_ambient`): what the game knows about the
//! moment - the weather, the time and season, the surface under each wheel of the player's
//! bus, the trees and houses around the listener, how open the bus is - turned into the
//! physical quantities `omsi_audio::ambient` synthesises from, once a frame.
//!
//! It never doubles what OMSI 2's own sounds already play: the rain in the street is
//! `Sounds\rain_outside.wav` (`ambience.rs`), and a bus whose sound configuration has its
//! own rain on the roof, wet-road hiss or rolling noise keeps them (see [`OwnSounds`]).
//!
//! It also tells the bus's scripts what each wheel rolls on (`Axle_SurfaceID_<i>_L/_R`, as
//! Omsi.exe does), whether the ambience is on or not.

use glam::{DVec3, Vec3};
use omsi_audio::ambient::{AmbientParams, Surface, WheelInput, MAX_WHEELS};
use omsi_audio::AudioEngine;
use std::path::PathBuf;

/// Which of the ambience's sounds the player's bus already makes itself.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OwnSounds {
    /// Rain on its roof (an entry on `PrecipRate` / `PrecipType`, OMSI's `regen.wav`).
    pub rain_roof: bool,
    /// Wet-road hiss (an entry on `StreetCond`, OMSI's `WetLane_*.wav`).
    pub wet_hiss: bool,
    /// Rolling noise of the tyres (a file named for it).
    pub rolling: bool,
}

impl OwnSounds {
    pub fn of(cfg: &omsi_vehicle::SoundCfg) -> OwnSounds {
        let mut own = OwnSounds::default();
        for s in &cfg.sounds {
            let vars = s.vol_curves.iter().map(|c| c.variable.as_str()).chain(s.conditions.iter().map(|c| c.variable.as_str()));
            for v in vars {
                if v.eq_ignore_ascii_case("PrecipRate") || v.eq_ignore_ascii_case("PrecipType") {
                    own.rain_roof = true;
                }
                if v.eq_ignore_ascii_case("StreetCond") {
                    own.wet_hiss = true;
                }
            }
            let file = s.file.replace('\\', "/").to_ascii_lowercase();
            let name = file.rsplit('/').next().unwrap_or("");
            if name.contains("wetlane") {
                own.wet_hiss = true;
            }
            // (`rollen.wav` of the AI cars, `Rollgeraeusch`, `Abrollen`, `Reifen` - not the
            // destination sign's `Rollband`)
            if name.starts_with("rollen") || ["rollger", "abroll", "reifen", "tyre", "tire", "rolling"].iter().any(|k| name.contains(k)) {
                own.rolling = true;
            }
        }
        own
    }
}

/// What the frame hands over (window and recording alike).
pub struct Moment<'a> {
    pub world: Option<&'a crate::scene::World>,
    pub weather: Option<&'a omsi_content::weather::Weather>,
    pub clock: &'a omsi_sim::SimClock,
    pub sun_elevation: f32,
    /// The roads' wetness (0 … 1, as the renderer's puddles take it).
    pub wetness: f32,
    /// The listener: where, facing which way, in the cab or not.
    pub ear: DVec3,
    pub right: Vec3,
    pub inside: bool,
    pub player: Option<&'a mut omsi_sim::VehicleInstance>,
    /// AI vehicles within earshot of the listener.
    pub traffic_near: usize,
    /// The echo zone's mix at the listener (a tunnel, an underpass).
    pub reverb_mix: f32,
    pub dt: f32,
}

pub struct AmbientSound {
    pub enabled: bool,
    pub volume: f32,
    /// The player's bus whose own sounds were looked at, and what it has.
    own_of: Option<PathBuf>,
    own: OwnSounds,
    surroundings: crate::scene::Surroundings,
    surroundings_at: Option<(DVec3, f32)>,
    birds_sent: bool,
    /// The wheels' surfaces now (for the log and the debug line).
    pub surfaces: [Option<Surface>; MAX_WHEELS],
    pub last: String,
}

impl AmbientSound {
    pub fn new(enabled: bool, volume: f32) -> AmbientSound {
        AmbientSound {
            enabled,
            volume,
            own_of: None,
            own: OwnSounds::default(),
            surroundings: Default::default(),
            surroundings_at: None,
            birds_sent: false,
            surfaces: [None; MAX_WHEELS],
            last: String::new(),
        }
    }

    /// One frame.
    pub fn update(&mut self, audio: &AudioEngine, mut m: Moment) {
        if !self.birds_sent && self.enabled {
            self.birds_sent = true;
            crate::ambient_assets::send_birds(audio);
        }
        let mut wheels = [WheelInput::default(); MAX_WHEELS];
        let mut bus_speed = 0.0f32;
        let mut open = 0.0;
        if let Some(v) = m.player.as_deref_mut() {
            self.look_at_own_sounds(v);
            bus_speed = (v.physics.velocity_kmh() / 3.6).abs();
            open = v.var("Snd_OutsideVol").unwrap_or(0.0);
            // a road the weather has covered in snow (OMSI's StreetCond over 1): the tyres
            // roll on the snow, whatever the texture under it is
            let snowy = m.weather.is_some_and(|w| w.snow_on_road || crate::weather_setup::precip_of(w).0 == 2);
            // (and snow lying on the land: the verges and fields are deep in it)
            let lying = m.weather.is_some_and(|w| w.snow);
            if let Some(w) = m.world {
                wheels = self.wheels(w, v, &m.ear, m.right, m.inside, m.wetness, bus_speed, (snowy, lying));
            }
        } else {
            self.surfaces = [None; MAX_WHEELS];
        }
        if !self.enabled || !audio.enabled {
            audio.set_ambient(AmbientParams { enabled: false, ..Default::default() });
            return;
        }
        // the trees and houses around: twice a second, or when the listener jumped
        let again = self.surroundings_at.is_none_or(|(at, t)| t > 0.5 || (at - m.ear).length() > 30.0);
        if again {
            if let Some(w) = m.world {
                self.surroundings = w.surroundings(m.ear, m.right);
            }
            self.surroundings_at = Some((m.ear, 0.0));
        } else if let Some((_, t)) = self.surroundings_at.as_mut() {
            *t += m.dt;
        }
        let s = self.surroundings;
        let p = self.params(&m, s, wheels, bus_speed, open);
        self.last = format!(
            "wind {:.1} m/s, z0 {:.2}, sky {:.2}, rain {:.1} mm/h, wet {:.2}, day {:.0}, foliage {:.2}, urban {:.2}, sun {:.0}°, inside {} open {:.2}, wheels {}",
            p.wind_10m,
            p.roughness,
            p.sky_open,
            p.rain_mm_h,
            p.wetness,
            p.day_of_year,
            p.foliage,
            p.urban,
            p.sun_elevation,
            p.inside,
            p.open,
            self.surfaces.iter().map(|s| s.map(|s| s.name()).unwrap_or("-")).collect::<Vec<_>>().join("/")
        );
        audio.set_ambient(p);
    }

    fn params(&self, m: &Moment, s: crate::scene::Surroundings, wheels: [WheelInput; MAX_WHEELS], bus_speed: f32, open: f32) -> AmbientParams {
        let day = m.clock.day_of_year as f32;
        let (wind_dir, wind) = m.weather.map(|w| w.wind).unwrap_or((0.0, 0.0));
        let (kind, rate) = m.weather.map(crate::weather_setup::precip_of).unwrap_or((0, 0.0));
        let rain_mm_h = rain_rate(rate);
        let temperature = m.weather.map(|w| w.temp.0).unwrap_or(15.0);
        let snow_lying = m.weather.is_some_and(|w| w.snow || w.snow_on_road);
        let convective = m.weather.is_some_and(|w| {
            let c = w.clouds.0.to_ascii_lowercase();
            (c.contains("cumulus 3") || c.contains("cumulonimbus") || c.contains("gewitter")) && rate > 0.45
        });
        let (leaf_amount, leaf_dryness) = leaves(day, snow_lying);
        // the bus moves through the air: what rushes past it is its speed against the wind
        let h = (wind_dir as f64).to_radians();
        let wind_v = DVec3::new(h.sin(), h.cos(), 0.0) * wind as f64;
        let air_speed = m
            .player
            .as_ref()
            .map(|v| {
                let hd = v.heading.to_radians();
                let bus_v = DVec3::new(hd.sin(), hd.cos(), 0.0) * bus_speed as f64;
                // (OMSI's wind blows *from* its direction)
                (bus_v + wind_v).length() as f32
            })
            .unwrap_or(0.0);
        // the roughness of the surroundings: a town's houses and a park's trees slow the
        // wind near the ground (z0 ~ 1 m), open fields hardly (0.03 m)
        let roughness = 0.03 + 1.2 * s.urban.max(0.6 * s.foliage);
        AmbientParams {
            enabled: true,
            volume: self.volume,
            wind_10m: wind,
            roughness,
            air_speed: if m.inside { air_speed } else { 0.0 },
            bus_speed,
            foliage: s.foliage,
            foliage_balance: s.balance,
            leaf_amount,
            leaf_dryness,
            rain_mm_h: if kind == 1 { rain_mm_h } else { 0.0 },
            snowing: kind == 2,
            snow_cover: if snow_lying { 1.0 } else { 0.0 },
            wetness: m.wetness,
            temperature,
            sun_elevation: m.sun_elevation,
            hour: m.clock.hour(),
            day_of_year: day,
            urban: s.urban,
            traffic: (m.traffic_near as f32 / 25.0).min(1.0),
            convective,
            inside: m.inside,
            open: open.clamp(0.0, 1.0),
            sky_open: (1.0 - m.reverb_mix).clamp(0.0, 1.0),
            roof_rain: !self.own.rain_roof,
            glass_rain: true,
            wheels,
        }
    }

    /// The bus's own sound configuration, looked at once per bus.
    fn look_at_own_sounds(&mut self, v: &omsi_sim::VehicleInstance) {
        let path = v.ty.def.sound.as_ref().map(|rel| omsi_cfg::resolve_path(v.ty.def.dir(), rel));
        if path == self.own_of {
            return;
        }
        let mut own = path.as_ref().and_then(|p| omsi_vehicle::SoundCfg::load(p).ok()).map(|c| OwnSounds::of(&c)).unwrap_or_default();
        // (an articulated bus's rear part may carry them)
        for t in &v.trailers {
            if let Some(rel) = &t.ty.def.sound {
                if let Ok(c) = omsi_vehicle::SoundCfg::load(&omsi_cfg::resolve_path(t.ty.def.dir(), rel)) {
                    let o = OwnSounds::of(&c);
                    own = OwnSounds { rain_roof: own.rain_roof || o.rain_roof, wet_hiss: own.wet_hiss || o.wet_hiss, rolling: own.rolling || o.rolling };
                }
            }
        }
        log::info!("ambience: the bus's own sounds - rain on the roof {}, wet-road hiss {}, rolling noise {}", own.rain_roof, own.wet_hiss, own.rolling);
        self.own = own;
        self.own_of = path;
    }

    /// The player's bus's tyres as the listener hears them: each side of the front axle and
    /// of the rear ones, on the surface under it (also told to the bus's scripts).
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn wheels(&mut self, w: &crate::scene::World, v: &mut omsi_sim::VehicleInstance, ear: &DVec3, right: Vec3, inside: bool, wetness: f32, speed: f32, (snowy, lying): (bool, bool)) -> [WheelInput; MAX_WHEELS] {
        let mut out = [WheelInput::default(); MAX_WHEELS];
        let Some(rb) = v.rigid.as_ref() else {
            self.surfaces = [None; MAX_WHEELS];
            return out;
        };
        let rot = v.body_rotation();
        let front_y = rb.wheels.iter().map(|w| w.attach.y).fold(f32::MIN, f32::max);
        // (group, contact, surface id, load share, axle, left)
        let mut found: Vec<(usize, DVec3, u8, f32, usize, bool)> = Vec::with_capacity(rb.wheels.len());
        for (k, wh) in rb.wheels.iter().enumerate() {
            let hub = v.position + rot.transform_vector3(wh.attach).as_dvec3();
            let contact = DVec3::new(hub.x, hub.y, if wh.ground_seen { wh.ground_z } else { hub.z - wh.radius as f64 });
            let id = w.surface_under(contact).unwrap_or(0);
            let left = wh.attach.x < 0.0;
            let front = wh.attach.y >= front_y - 0.5;
            let group = (if front { 0 } else { 2 }) + if left { 0 } else { 1 };
            let load = if wh.rest_load > 0.0 { wh.load / wh.rest_load } else { 1.0 };
            found.push((group, contact, id, load, rb.wheel_axle.get(k).copied().unwrap_or(0), left));
        }
        // the scripts' `Axle_SurfaceID_<axle>_L/_R`
        for (_, _, id, _, axle, left) in &found {
            let name = format!("Axle_SurfaceID_{axle}_{}", if *left { "L" } else { "R" });
            v.set_var(&name, *id as f32);
        }
        let water_film = wetness.clamp(0.0, 1.0) * WATER_FILM_MM;
        for g in 0..MAX_WHEELS {
            let mine: Vec<_> = found.iter().filter(|f| f.0 == g).collect();
            let Some(first) = mine.first() else {
                self.surfaces[g] = None;
                continue;
            };
            let n = mine.len() as f64;
            let contact = mine.iter().map(|f| f.1).sum::<DVec3>() / n;
            let load = mine.iter().map(|f| f.3).sum::<f32>() / mine.len() as f32;
            let surface = match Surface::from_omsi(first.2) {
                Surface::Asphalt | Surface::Concrete | Surface::Cobble if snowy => Surface::Snow,
                Surface::Dirt | Surface::Grass | Surface::Gravel if lying => Surface::DeepSnow,
                s => s,
            };
            self.surfaces[g] = Some(surface);
            let wet_road = w.wet_road_at(contact.x, contact.y, wetness);
            let water = crate::puddles::water_at(contact.x, contact.y, wet_road);
            let puddle_mm = water.puddle * water.depth * PUDDLE_DEPTH_MM;
            let (gain_l, gain_r) = wheel_gains(contact, *ear, right, inside, mine.len());
            out[g] = WheelInput {
                speed,
                surface,
                water_mm: water_film,
                puddle_mm,
                load,
                gain_l,
                gain_r,
                through_body: inside,
                skip_base: self.own.rolling,
                skip_wet: self.own.wet_hiss,
            };
        }
        out
    }
}

/// A soaked road's water film (mm): a millimetre stands on asphalt in steady rain.
const WATER_FILM_MM: f32 = 1.0;
/// The deepest of the renderer's puddles (mm).
const PUDDLE_DEPTH_MM: f32 = 25.0;

/// How a group of `count` tyres at `contact` reaches the ears: from outside, the inverse
/// distance beyond a couple of metres and OMSI's pan; from inside, through the floor - the
/// wheel arch beside the driver is near, the rear axle ten metres back.
fn wheel_gains(contact: DVec3, ear: DVec3, right: Vec3, inside: bool, count: usize) -> (f32, f32) {
    let d = (contact - ear).as_vec3();
    let n = (count as f32).sqrt();
    if inside {
        let g = omsi_audio::mixer::distance_gain(3.0, d.length()) * 0.4 * n;
        let s = d.normalize_or_zero().dot(right).clamp(-1.0, 1.0);
        return (g * (1.0 - 0.3 * s), g * (1.0 + 0.3 * s));
    }
    let g = omsi_audio::mixer::distance_gain(2.5, d.length()) * n;
    let l = omsi_audio::Listener { forward: right.cross(Vec3::Z) * -1.0, right, ..Default::default() };
    let (pl, pr) = omsi_audio::mixer::pan_gains(&l, d);
    (g * pl, g * pr)
}

/// OMSI's precipitation rate (0 … 1, the weather's byte / 255) as a rain rate in mm/h: the
/// METAR presets put light rain at 70, moderate at 150, heavy at 230, which the
/// meteorologists' classes (light < 2.5, moderate 2.5 - 7.6, heavy > 7.6 mm/h) meet at
/// about 1, 3 and 11 mm/h.
pub fn rain_rate(rate: f32) -> f32 {
    if rate <= 0.0 {
        0.0
    } else {
        0.3 * (4.0 * rate.min(1.2)).exp()
    }
}

/// The leaves on the trees on day `day` of the year (north of the equator): (how many,
/// how dry). Out in late April, dry and falling from late September, gone by mid-November.
pub fn leaves(day: f32, snow: bool) -> (f32, f32) {
    use omsi_audio::ambient::weather::smoothstep;
    let on = smoothstep(105.0, 135.0, day) * (1.0 - smoothstep(290.0, 320.0, day));
    let dry = smoothstep(255.0, 295.0, day);
    let amount = if snow { on * 0.2 } else { on };
    (amount, dry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_classes_of_the_metar_presets() {
        assert_eq!(rain_rate(0.0), 0.0);
        let light = rain_rate(70.0 / 255.0);
        let moderate = rain_rate(150.0 / 255.0);
        let heavy = rain_rate(230.0 / 255.0);
        assert!(light < 2.5 && moderate > 2.5 && moderate < 7.6 && heavy > 7.6, "{light} {moderate} {heavy}");
    }

    #[test]
    fn leaves_through_the_year() {
        assert_eq!(leaves(30.0, false).0, 0.0);
        assert!(leaves(180.0, false).0 > 0.99 && leaves(180.0, false).1 < 0.01);
        let (amount, dry) = leaves(285.0, false);
        assert!(amount > 0.5 && dry > 0.8, "{amount} {dry}");
        assert_eq!(leaves(340.0, false).0, 0.0);
    }

    #[test]
    fn the_own_sounds_of_a_stock_bus() {
        let cfg = omsi_cfg::CfgFile::from_str(
            "s.cfg",
            "[sound]\r\n..\\..\\..\\Sounds\\WetLane_1.wav\r\n1\r\n\r\n[volcurve]\r\nStreetCond\r\n\r\n[sound]\r\nregen.wav\r\n1\r\n\r\n[volcurve]\r\nPrecipRate\r\n\r\n[sound]\r\nmotor.wav\r\n1\r\n",
        );
        let own = OwnSounds::of(&omsi_vehicle::SoundCfg::parse(&cfg));
        assert_eq!(own, OwnSounds { rain_roof: true, wet_hiss: true, rolling: false });
        let cfg = omsi_cfg::CfgFile::from_str("s.cfg", "[sound]\r\nRollgeraeusch.wav\r\n1\r\n");
        assert!(OwnSounds::of(&omsi_vehicle::SoundCfg::parse(&cfg)).rolling);
        let cfg = omsi_cfg::CfgFile::from_str("s.cfg", "[sound]\r\nRollband_auf.wav\r\n1\r\n");
        assert!(!OwnSounds::of(&omsi_vehicle::SoundCfg::parse(&cfg)).rolling, "the destination sign's roller");
    }

    #[test]
    fn a_wheel_beside_the_ear_is_louder_than_one_far_off() {
        let ear = DVec3::ZERO;
        let near = wheel_gains(DVec3::new(2.0, 0.0, 0.0), ear, Vec3::X, false, 2);
        let far = wheel_gains(DVec3::new(20.0, 0.0, 0.0), ear, Vec3::X, false, 2);
        assert!(near.1 > 5.0 * far.1);
        // to the right: the right ear has more
        assert!(near.1 > near.0);
        let cab = wheel_gains(DVec3::new(1.0, 1.0, -1.0), ear, Vec3::X, true, 2);
        assert!(cab.0 < near.0 && cab.1 < near.1, "through the floor");
    }
}

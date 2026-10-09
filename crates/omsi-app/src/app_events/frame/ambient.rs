//! openOMSI's ambience in the window's frame (see `ambient_sound`).

use super::*;

/// AI vehicles within this distance of the listener count as the traffic heard (m).
const TRAFFIC_EARSHOT: f64 = 300.0;

impl App {
    /// The ambience's quantities of this frame: the weather, the time, the trees and houses
    /// around the camera, the surfaces under the player's wheels.
    pub(super) fn frame_ambient(&mut self, dt: f32, daylight: &omsi_sim::Daylight) {
        let __t = Instant::now();
        let (Some(a), Some(cam)) = (self.sound.audio.as_ref(), self.camera.as_ref()) else { return };
        let amb = self.sound.ambient_sound.get_or_insert_with(|| crate::ambient_sound::AmbientSound::new(self.settings.ambient, self.settings.vol_ambient));
        amb.enabled = self.settings.ambient && !self.paused;
        amb.volume = self.settings.vol_ambient;
        let ear = cam.position;
        let traffic_near = self
            .session
            .traffic
            .as_ref()
            .map(|t| t.cars.iter().filter(|c| (c.vehicle.position - ear).length() < TRAFFIC_EARSHOT).count())
            .unwrap_or(0);
        let reverb_mix = self.world.as_ref().map(|w| w.reverb_at(ear).1).unwrap_or(0.0);
        let wetness = puddles::road_wetness(self.session.wetness, self.session.weather.as_ref().is_some_and(|w| w.snow));
        amb.update(
            a,
            crate::ambient_sound::Moment {
                world: self.world.as_deref(),
                weather: self.session.weather.as_ref(),
                clock: &self.clock,
                sun_elevation: daylight.altitude_deg,
                wetness,
                ear,
                right: cam.right(),
                inside: self.cam.in_cab,
                player: self.player.as_mut().map(|p| &mut p.vehicle),
                traffic_near,
                reverb_mix,
                dt,
            },
        );
        if let Some(every) = debug_sound_every() {
            static LAST: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
            let bucket = (self.clock.time / every as f64) as u32;
            if LAST.swap(bucket, std::sync::atomic::Ordering::Relaxed) != bucket {
                let levels = a.ambient_levels();
                let parts: Vec<String> = omsi_audio::ambient::PARTS.iter().zip(levels).filter(|(_, l)| *l > 1.0e-5).map(|(k, l)| format!("{k} {:.0} dB", 20.0 * l.log10())).collect();
                log::info!("sound: ambience - {} ({})", amb.last, parts.join(", "));
            }
        }
        *self.perf.profile.entry("ambience").or_default() += __t.elapsed().as_secs_f64();
    }
}

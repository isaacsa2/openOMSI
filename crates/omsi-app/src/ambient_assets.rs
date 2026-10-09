//! The ambience's recordings, built into the program (`assets/sounds/ambient`, licences in
//! its CREDITS.md): birdsong, scattered by day. Everything else of the ambience is
//! synthesised.

use std::sync::Arc;

/// The bird recordings (FLAC, mono 32 kHz, high-passed at 180 Hz against wind and traffic
/// rumble): a morning flock of sparrows and pigeons, a blackbird, a chaffinch, a redstart.
const BIRDS: &[(&str, &[u8])] = &[
    ("birds_morning_flock", include_bytes!("../../../assets/sounds/ambient/birds_morning_flock.flac")),
    ("birds_blackbird", include_bytes!("../../../assets/sounds/ambient/birds_blackbird.flac")),
    ("birds_chaffinch", include_bytes!("../../../assets/sounds/ambient/birds_chaffinch.flac")),
    ("birds_redstart", include_bytes!("../../../assets/sounds/ambient/birds_redstart.flac")),
];

/// Decode the bird recordings on a thread of their own and hand them to the ambience.
pub fn send_birds(audio: &omsi_audio::AudioEngine) {
    if BIRDS.is_empty() || !audio.enabled {
        return;
    }
    audio.set_ambient_birds_later(|| {
        BIRDS
            .iter()
            .filter_map(|(name, bytes)| match omsi_audio::wav::parse_compressed(bytes) {
                Ok(w) => Some(Arc::new(omsi_audio::Clip { sample_rate: w.sample_rate, channels: w.channels, samples: w.samples })),
                Err(e) => {
                    log::warn!("ambience: {name}: {e}");
                    None
                }
            })
            .collect()
    });
}

#[cfg(test)]
mod tests {
    /// Every recording decodes, at a sensible level, without clipping.
    #[test]
    fn the_recordings_decode() {
        for (name, bytes) in super::BIRDS {
            let w = omsi_audio::wav::parse_compressed(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let secs = w.samples.len() as f32 / w.channels as f32 / w.sample_rate as f32;
            let peak = w.samples.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
            assert!(secs > 20.0, "{name}: {secs} s");
            assert!(peak > 3000 && peak < 32_000, "{name}: peak {peak}");
        }
    }
}

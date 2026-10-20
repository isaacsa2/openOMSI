//! The output device's thread: it owns the cpal stream (on some platforms a stream may not
//! leave the thread that made it, and the engine must be free to move to the simulation's
//! thread), opens it again when asked, and looks every two seconds whether the system's
//! default output changed.

use super::render::{MixerCell, Reverb, Shared};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The engine's end of the device thread.
pub(super) struct DeviceThread {
    open: Option<mpsc::Sender<()>>,
    opened: mpsc::Receiver<Option<String>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl DeviceThread {
    /// The thread, with the stream opened on the default device: `None` when no thread
    /// could be started.
    pub(super) fn start(shared: Arc<Shared>, mixer: Arc<MixerCell>, reopen: Arc<AtomicBool>) -> Option<DeviceThread> {
        let (open_tx, open_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("audio device".into())
            .spawn(move || run(open_rx, done_tx, &shared, &mixer, &reopen))
            .ok()?;
        Some(DeviceThread { open: Some(open_tx), opened: done_rx, join: Some(join) })
    }

    /// Play on the system's default output device from now on: the name of the device
    /// played on, `None` when no stream plays.
    pub(super) fn open(&self) -> Option<String> {
        self.open.as_ref()?.send(()).ok()?;
        self.opened.recv().ok().flatten()
    }
}

impl Drop for DeviceThread {
    /// The stream is closed before the engine is gone (playback stops when it is dropped).
    fn drop(&mut self) {
        self.open.take();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run(open: mpsc::Receiver<()>, opened: mpsc::Sender<Option<String>>, shared: &Arc<Shared>, mixer: &Arc<MixerCell>, reopen: &Arc<AtomicBool>) {
    let mut stream: Option<cpal::Stream> = None;
    // The default device's name the watcher saw last (from the first stream on; no stream
    // at the start, no watching).
    let mut watched: Option<String> = None;
    let mut next_look = Instant::now() + Duration::from_secs(2);
    loop {
        match open.recv_timeout(next_look.saturating_duration_since(Instant::now())) {
            Ok(()) => {
                // (the old stream first: some drivers give a device to one stream at a time)
                stream = None;
                let name = open_default(shared, mixer, reopen).map(|(s, name)| {
                    stream = Some(s);
                    name
                });
                if watched.is_none() {
                    if let Some(n) = &name {
                        watched = Some(n.clone());
                        next_look = Instant::now() + Duration::from_secs(2);
                    }
                }
                let _ = opened.send(name);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                next_look = Instant::now() + Duration::from_secs(2);
                if let Some(current) = watched.as_mut() {
                    let name = cpal::default_host().default_output_device().and_then(|d| d.name().ok()).unwrap_or_default();
                    if name != *current {
                        *current = name;
                        reopen.store(true, Ordering::Relaxed);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stream);
}

/// A stream on the system's default output device, mixing from `mixer`, with the device's name.
fn open_default(shared: &Arc<Shared>, mixer: &Arc<MixerCell>, reopen: &Arc<AtomicBool>) -> Option<(cpal::Stream, String)> {
    let host = cpal::default_host();
    let Some(dev) = host.default_output_device() else {
        log::warn!("audio: no output device");
        return None;
    };
    let name = dev.name().unwrap_or_default();
    let cfg = match dev.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("audio: no output config on {name}: {e}");
            return None;
        }
    };
    let (rate, ch) = (cfg.sample_rate().0, cfg.channels().max(1) as usize);
    shared.sample_rate.store(rate, Ordering::Relaxed);
    shared.channels.store(ch, Ordering::Relaxed);

    // The callback must use the format selected by the output device. Some Windows and
    // mobile devices default to 16-bit integer samples rather than floating point.
    // Integer callbacks use a fixed stack buffer: no allocations or locks in the callback.
    let config = cfg.config();
    let cell = mixer.clone();
    let lost = reopen.clone();
    let mut spare = Reverb::ready(rate, ch);
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => dev.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                if cell.try_with(|m| {
                    m.adopt_reverb(&mut spare);
                    m.render(data);
                }).is_none() {
                    data.fill(0.0);
                }
            },
            move |e| report_device_error(&lost, e),
            None,
        ),
        cpal::SampleFormat::I16 => dev.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                render_converted(&cell, &mut spare, data, ch, 0, to_i16);
            },
            move |e| report_device_error(&lost, e),
            None,
        ),
        cpal::SampleFormat::U16 => dev.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                render_converted(&cell, &mut spare, data, ch, 32_768, to_u16);
            },
            move |e| report_device_error(&lost, e),
            None,
        ),
        other => {
            log::warn!("audio: unsupported output sample format on {name}: {other:?}");
            return None;
        }
    };
    match stream {
        Ok(s) => {
            if let Err(e) = s.play() {
                log::warn!("audio: cannot start stream on {name}: {e}");
                return None;
            }
            log::info!(
                "audio: playing on {name} ({} Hz, {} channels, {:?})",
                rate, cfg.channels(), cfg.sample_format()
            );
            Some((s, name))
        }
        Err(e) => {
            log::warn!("audio: cannot open stream on {name}: {e}");
            None
        }
    }
}

fn report_device_error(reopen: &AtomicBool, e: cpal::StreamError) {
    if matches!(e, cpal::StreamError::DeviceNotAvailable) {
        reopen.store(true, Ordering::Relaxed);
    }
    log::warn!("audio stream error: {e}");
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32768.0) as i16
}

fn to_u16(x: f32) -> u16 {
    ((x.clamp(-1.0, 1.0) + 1.0) * 32767.5).round() as u16
}

/// Convert the mix into the device's integer format without allocating in the callback.
fn render_converted<T: Copy>(
    cell: &MixerCell,
    spare: &mut Reverb,
    data: &mut [T],
    channels: usize,
    silence: T,
    convert: fn(f32) -> T,
) {
    const SCRATCH: usize = 4096;
    // Keep every chunk aligned to complete frames, including devices with >2 channels.
    let chunk_size = SCRATCH / channels.max(1) * channels.max(1);
    if chunk_size == 0 {
        data.fill(silence);
        return;
    }
    if cell.try_with(|m| {
        m.adopt_reverb(spare);
        let mut scratch = [0.0f32; SCRATCH];
        for chunk in data.chunks_mut(chunk_size) {
            m.render(&mut scratch[..chunk.len()]);
            for (out, sample) in chunk.iter_mut().zip(&scratch) {
                *out = convert(*sample);
            }
        }
    }).is_none() {
        data.fill(silence);
    }
}

#[cfg(test)]
mod tests {
    use super::{to_i16, to_u16};

    #[test]
    fn integer_formats_preserve_silence_and_extremes() {
        assert_eq!(to_i16(-1.0), i16::MIN);
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), i16::MAX);
        assert_eq!(to_u16(-1.0), 0);
        assert_eq!(to_u16(0.0), 32_768);
        assert_eq!(to_u16(1.0), u16::MAX);
    }
}

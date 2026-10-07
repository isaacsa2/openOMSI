//! Bounded export of OMSI_PROFILE's cumulative timers, not another profiler.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

type Stages = BTreeMap<String, f64>;
const MAX_FRAMES: usize = 120_000;

pub(crate) fn duration(s: &str) -> Result<u32, String> {
    match s.parse::<u32>() {
        Ok(n @ (10 | 30 | 60)) => Ok(n),
        _ => Err("capture duration must be 10, 30 or 60 seconds".into()),
    }
}

#[derive(Serialize)]
pub(crate) struct Frame {
    frame_number: u32,
    timestamp_seconds: f64,
    frame_time_ms: f64,
    /// Inclusive stages: parents and their sub-stages must not be added together.
    stages_ms: Stages,
    #[serde(skip_serializing_if = "Option::is_none")]
    graphics: Option<Graphics>,
}

#[derive(Serialize)]
struct Graphics {
    graphics_mode: String,
    msaa: u32,
    anisotropy: u16,
    ssao: bool,
    render_scale: f32,
    shadows: bool,
    shadow_size: u32,
    mirror_size: u32,
    mirror_refresh: String,
    cloud_quality: String,
    gpu_texture_compression: String,
    max_obj_dist: f32,
    lan_active: bool,
}

#[derive(Serialize, Debug)]
pub(crate) struct Summary {
    frames: usize,
    measured_seconds: f64,
    average_fps: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    worst_frame_ms: f64,
    frames_over_16_7_ms: usize,
    frames_over_33_3_ms: usize,
    frames_over_50_ms: usize,
    frames_over_100_ms: usize,
    frame_limit_reached: bool,
    stage_average_ms: Stages,
}

#[derive(Serialize, Clone, Default)]
struct Memory {
    physical_memory_bytes: Option<u64>,
    gpu_textures_bytes: Option<u64>,
    gpu_meshes_bytes: Option<u64>,
}

/// GPU aggregates are deltas of existing query results. They can cover fewer
/// frames than the CPU capture and must not be mistaken for per-frame GPU time.
#[derive(Serialize)]
struct GpuPass {
    pass: String,
    average_ms: f64,
    measured_frames: u32,
}

pub(crate) struct Capture {
    seconds: u32,
    delay: u32,
    output: PathBuf,
    benchmark: Option<String>,
    ready_at: Option<Instant>,
    elapsed: f64,
    previous: Option<Stages>,
    frames: Vec<Frame>,
    gpu_start: BTreeMap<String, (f64, u32)>,
    memory_start: Memory,
}

impl Capture {
    pub(crate) fn new(
        seconds: u32,
        delay: u32,
        output: PathBuf,
        benchmark: Option<String>,
    ) -> Self {
        Self {
            seconds,
            delay,
            output,
            benchmark,
            ready_at: None,
            elapsed: 0.0,
            previous: None,
            frames: Vec::new(),
            gpu_start: BTreeMap::new(),
            memory_start: Memory::default(),
        }
    }

    /// Returns true when the bounded capture is complete. Counter resets after a
    /// renderer fallback are rebased per-stage rather than becoming negative.
    fn sample(&mut self, frame_number: u32, dt: f64, totals: Stages) -> bool {
        let Some(previous) = self.previous.replace(totals.clone()) else { return false };
        if !dt.is_finite() || dt <= 0.0 { return false; }
        let stages_ms = totals.into_iter().map(|(k, total)| {
            let prev = previous.get(&k).copied().unwrap_or(total);
            let delta = if total >= prev { total - prev } else { total };
            (k, delta.max(0.0) * 1000.0)
        }).collect();
        self.elapsed += dt;
        self.frames.push(Frame { frame_number, timestamp_seconds: self.elapsed,
            frame_time_ms: dt * 1000.0, stages_ms, graphics: None });
        self.elapsed >= self.seconds as f64 || self.frames.len() >= MAX_FRAMES
    }

    fn write(self, gpu_end: Vec<(String, f64, u32)>, memory_end: Memory) -> anyhow::Result<()> {
        let gpu: Vec<_> = gpu_end.into_iter().filter_map(|(pass, avg, count)| {
            let (start_ms, start_count) = self.gpu_start.get(&pass).copied().unwrap_or((0.0, 0));
            let n = count.checked_sub(start_count)?;
            (n > 0).then(|| GpuPass { pass, average_ms: (avg * count as f64 - start_ms).max(0.0) / n as f64,
                measured_frames: n })
        }).collect();
        let summary = summarize(&self.frames, self.frames.len() >= MAX_FRAMES);
        let json = serde_json::json!({"schema_version": 1,
            "version": crate::startup::VERSION, "build": crate::startup::BUILD,
            "benchmark": self.benchmark,
            "requested_seconds": self.seconds, "warmup_seconds": self.delay,
            "summary": summary, "gpu_passes": gpu,
            "memory_start": self.memory_start, "memory_end": memory_end,
            "frames": self.frames});
        write_pair(&self.output, &summary, &json)
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    // Nearest rank, including both endpoints for a one-frame capture.
    sorted[((p * sorted.len() as f64).ceil() as usize).saturating_sub(1).min(sorted.len() - 1)]
}

fn summarize(frames: &[Frame], capped: bool) -> Summary {
    let mut times: Vec<_> = frames.iter().map(|f| f.frame_time_ms).collect();
    times.sort_by(f64::total_cmp);
    let seconds = times.iter().sum::<f64>() / 1000.0;
    let mut stages = Stages::new();
    for frame in frames { for (k, v) in &frame.stages_ms { *stages.entry(k.clone()).or_default() += v; } }
    for v in stages.values_mut() { *v /= frames.len().max(1) as f64; }
    Summary { frames: frames.len(), measured_seconds: seconds,
        average_fps: if seconds > 0.0 { frames.len() as f64 / seconds } else { 0.0 },
        p50_ms: percentile(&times, 0.5), p95_ms: percentile(&times, 0.95), p99_ms: percentile(&times, 0.99),
        worst_frame_ms: times.last().copied().unwrap_or(0.0),
        frames_over_16_7_ms: times.iter().filter(|&&t| t > 16.7).count(),
        frames_over_33_3_ms: times.iter().filter(|&&t| t > 33.3).count(),
        frames_over_50_ms: times.iter().filter(|&&t| t > 50.0).count(),
        frames_over_100_ms: times.iter().filter(|&&t| t > 100.0).count(),
        frame_limit_reached: capped, stage_average_ms: stages }
}

fn write_pair(dir: &Path, summary: &Summary, value: &serde_json::Value) -> anyhow::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let json_path = dir.join("performance.json");
    let text_path = dir.join("performance-summary.txt");
    // Reserve both outputs before writing: never replace a previous capture.
    let mut json_file = std::fs::OpenOptions::new().write(true).create_new(true).open(&json_path)?;
    let mut text_file = match std::fs::OpenOptions::new().write(true).create_new(true).open(&text_path) {
        Ok(f) => f,
        Err(e) => { drop(json_file); let _ = std::fs::remove_file(json_path); return Err(e.into()); }
    };
    let text = format!("openOMSI performance capture\nFrames: {}\nDuration: {:.3} s\nAverage FPS: {:.2}\np50 / p95 / p99: {:.3} / {:.3} / {:.3} ms\nWorst: {:.3} ms\nFrames > 16.7 / 33.3 / 50 / 100 ms: {} / {} / {} / {}\nFrame limit reached: {}\n\nStage averages (ms, inclusive; do not sum parent and child stages):\n{:#?}\n\nGPU data comes only from already enabled and supported timestamp queries. An empty GPU list means unavailable, not zero GPU cost. The stage named gpu is a CPU wait, not GPU execution time. Startup/streaming warm-up and partial captures must be considered when comparing runs.\n", summary.frames, summary.measured_seconds,
        summary.average_fps, summary.p50_ms, summary.p95_ms, summary.p99_ms, summary.worst_frame_ms,
        summary.frames_over_16_7_ms, summary.frames_over_33_3_ms, summary.frames_over_50_ms,
        summary.frames_over_100_ms, summary.frame_limit_reached, summary.stage_average_ms);
    serde_json::to_writer_pretty(&mut json_file, value)?;
    text_file.write_all(text.as_bytes())?;
    Ok(())
}

impl crate::App {
    pub(crate) fn capture_performance(&mut self, dt: f64) {
        let Some(mut capture) = self.capture.take() else { return };
        if self.world.is_none() || self.starting.is_some() {
            capture.ready_at = None;
            self.capture = Some(capture);
            return;
        }
        let ready = capture.ready_at.get_or_insert_with(Instant::now);
        if ready.elapsed().as_secs_f64() < capture.delay as f64 {
            self.capture = Some(capture);
            return;
        }
        let mut totals: Stages = self.profile.iter().map(|(&k, &v)| (k.to_string(), v)).collect();
        if let Some(r) = &self.renderer {
            totals.extend(r.stats.borrow().iter().map(|(&k, &v)| (format!("render.{k}"), v)));
        }
        let memory = || Memory { physical_memory_bytes: crate::memory::physical_memory(),
            gpu_textures_bytes: self.renderer.as_ref().zip(self.scene.as_ref()).map(|(r, s)| r.texture_bytes(s)),
            gpu_meshes_bytes: self.renderer.as_ref().zip(self.scene.as_ref()).map(|(r, s)| r.mesh_bytes(s)) };
        if capture.previous.is_none() {
            capture.memory_start = memory();
            if let Some(r) = &self.renderer {
                capture.gpu_start = r.gpu_pass_times().into_iter().map(|(k, avg, n)| (k, (avg * n as f64, n))).collect();
            }
            log::info!("performance capture started ({} s)", capture.seconds);
        }
        let done = capture.sample(self.total_frames, dt, totals);
        if let (Some(frame), Some(r)) = (capture.frames.last_mut(), self.renderer.as_ref()) {
            frame.graphics = Some(Graphics {
                graphics_mode: self.settings.graphics.clone(),
                msaa: r.options.msaa,
                anisotropy: self.settings.anisotropy,
                ssao: r.options.ssao,
                render_scale: r.options.render_scale,
                shadows: self.settings.shadows,
                shadow_size: self.settings.shadow_size,
                mirror_size: self.settings.mirror_size,
                mirror_refresh: self.settings.mirror_refresh.clone(),
                cloud_quality: self.settings.cloud_quality.clone(),
                gpu_texture_compression: self.settings.gpu_texture_compression.clone(),
                max_obj_dist: self.settings.max_obj_dist,
                lan_active: self.lan.is_some(),
            });
        }
        if !done {
            self.capture = Some(capture);
            return;
        }
        let gpu = self.renderer.as_ref().map(|r| r.gpu_pass_times()).unwrap_or_default();
        let memory = memory();
        let out = capture.output.clone();
        let benchmark = capture.benchmark.is_some();
        omsi_cfg::env::set_profile_capture(false);
        if let Some(r) = self.renderer.as_mut() {
            r.set_profiling(omsi_cfg::env::var_os("OMSI_PROFILE").is_some());
        }
        if benchmark {
            // The benchmark ends after its measured window. Write synchronously now, outside
            // that window, so process exit cannot race the export worker and lose the result.
            match capture.write(gpu, memory) {
                Ok(()) => {
                    log::info!("benchmark capture exported to {}", out.display());
                    self.args.exit_after = Some(self.started.elapsed().as_secs_f32() + 0.5);
                }
                Err(e) => log::warn!("could not export benchmark capture: {e:#}"),
            }
        } else {
            // An ordinary game keeps running; keep its potentially large JSON write off the UI
            // thread after sampling has ended.
            std::thread::spawn(move || match capture.write(gpu, memory) {
                Ok(()) => log::info!("performance capture exported to {}", out.display()),
                Err(e) => log::warn!("could not export performance capture: {e:#}"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_rank_and_thresholds_are_not_fps_averages() {
        let frames: Vec<_> = [10.0, 20.0, 40.0, 60.0, 120.0].into_iter().enumerate().map(|(i, ms)|
            Frame { frame_number: i as u32, timestamp_seconds: 0.0, frame_time_ms: ms, stages_ms: Stages::new(), graphics: None }).collect();
        let s = summarize(&frames, false);
        assert_eq!((s.p50_ms, s.p95_ms, s.p99_ms), (40.0, 120.0, 120.0));
        assert_eq!((s.frames_over_16_7_ms, s.frames_over_33_3_ms, s.frames_over_50_ms, s.frames_over_100_ms), (4, 3, 2, 1));
        assert!((s.average_fps - 20.0).abs() < 1e-9);
        assert_eq!(summarize(&[], false).average_fps, 0.0);
    }
    #[test]
    fn capture_subtracts_baseline_and_rebases_reset_counters() {
        let mut c = Capture::new(10, 0, PathBuf::new(), None);
        let stages = |v| BTreeMap::from([("render.cull".into(), v)]);
        assert!(!c.sample(1, 9.0, stages(10.0))); // warm-up is only the baseline
        assert!(!c.sample(2, 0.02, stages(10.002)));
        assert!((c.frames[0].stages_ms["render.cull"] - 2.0).abs() < 1e-9);
        assert!(!c.sample(3, 0.02, stages(0.001)));
        assert_eq!(c.frames[1].stages_ms["render.cull"], 1.0);
        assert!(c.sample(4, 10.0, stages(0.002)));
        assert_eq!(c.frames.len(), 3);
    }
    #[test]
    fn durations_and_invalid_frame_values_are_bounded() {
        assert_eq!(duration("30"), Ok(30));
        assert!(duration("31").is_err());
        let mut c = Capture::new(10, 0, PathBuf::new(), None);
        c.sample(0, 1.0, Stages::new());
        c.sample(1, f64::NAN, Stages::new());
        c.sample(2, -1.0, Stages::new());
        assert!(c.frames.is_empty());
    }
}

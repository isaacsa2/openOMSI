//! Waiting for the GPU and keeping its work moving: waits made of short slices on OpenGL
//! ([`wait_gpu`]), worker threads taking turns at the GL context, and the thread that polls
//! the device elsewhere.

use super::device_caps::gl_backend;
use std::sync::Arc;

/// Wait until the GPU has done `submission` (None: everything submitted so far).
///
/// On OpenGL wgpu holds the one GL context for the whole of a wait, and every other thread
/// that wants it meanwhile (a worker making a bus's textures) gives up after
/// a second with a panic - "Could not lock adapter context. This is most-likely a deadlock."
/// (wgpu-hal's WGL lock; #843: a slow chip took longer than that for a frame). There the
/// wait is made of short ones, and the context is free between them.
pub fn wait_gpu(device: &wgpu::Device, submission: Option<wgpu::SubmissionIndex>) -> Result<(), wgpu::PollError> {
    if !gl_backend() {
        return device.poll(wgpu::PollType::Wait { submission_index: submission, timeout: None }).map(|_| ());
    }
    loop {
        match device.poll(wgpu::PollType::Wait { submission_index: submission.clone(), timeout: Some(GL_WAIT_SLICE) }) {
            Err(wgpu::PollError::Timeout) => std::thread::yield_now(),
            r => return r.map(|_| ()),
        }
    }
}

/// The longest a single wait for the GPU holds the GL context (see [`wait_gpu`]).
const GL_WAIT_SLICE: std::time::Duration = std::time::Duration::from_millis(20);

/// On OpenGL, the GPU work of worker threads (textures and meshes of a bus made while the
/// world loads) goes one thread at a time: a dozen of them queueing for the GL context left
/// the last one waiting past wgpu's one second (#843). Elsewhere the device takes them all.
pub(super) fn gl_worker_turn() -> Option<std::sync::MutexGuard<'static, ()>> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    gl_backend().then(|| TURN.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(super) struct DevicePoller {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DevicePoller {
    pub(super) fn start(device: &wgpu::Device) -> Option<Self> {
        // Not on OpenGL: there every poll takes the one GL context, and whenever the thread
        // drawing held it for more than a second (a big shader linked while the world
        // loads, a slow chip's frame) this thread gave up with wgpu-hal's panic "Could not
        // lock adapter context" (#898, after #843). It is not needed there: every submit
        // of the frame runs the same upkeep (wgpu-core's `maintain` after `queue.submit`).
        if cfg!(target_arch = "wasm32") || gl_backend() || omsi_cfg::env::var_os("OMSI_NO_POLL_THREAD").is_some() {
            return None;
        }
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (device, flag) = (device.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("omsi-gpu-poll".into())
            .spawn(move || {
                let pause = std::time::Duration::from_millis(1);
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = device.poll(wgpu::PollType::Poll);
                    std::thread::sleep(pause);
                }
            })
            .ok()?;
        Some(Self { stop, thread: Some(thread) })
    }
}

impl Drop for DevicePoller {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

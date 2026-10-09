//! The frame submitted and shown from a thread of its own (OMSI_PRESENT_THREAD=1, OpenGL and
//! ANGLE). There wgpu turns the frame's commands into GL calls inside `submit` and presents
//! by a framebuffer blit and the swap, all under the GL context's lock: 12 ms and 1 ms of a
//! 44 ms frame on a Radeon R7 200 under ANGLE, next to nothing on Vulkan. From this thread
//! the window picture of one frame is submitted and shown while the game's thread steps the
//! next one (the bus, the traffic, the scripts).
//!
//! What the next frame writes before this thread has submitted would land ahead of the
//! frame's commands (a `write_buffer` goes in with the next submit). So whatever may write
//! into a place the frame still draws from waits for it first (`Renderer::settle`): the next
//! picture (mirrors included), a mesh put into a freed place of a page, a light map tile.
//! Writes that only change what a texture shows (a display, the navigator) may show one
//! frame early. The next frame's picture is acquired only when this one is shown (see
//! `SurfaceState::acquire`), so one frame is in flight as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

struct Job {
    commands: Vec<wgpu::CommandBuffer>,
    frame: wgpu::SurfaceTexture,
}

pub(crate) struct PresentThread {
    queue: wgpu::Queue,
    frames: Option<mpsc::SyncSender<Job>>,
    done: Mutex<mpsc::Receiver<()>>,
    /// A frame was sent and its present not waited for.
    pending: AtomicBool,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PresentThread {
    /// The thread where it is asked for and the device is OpenGL's (a submit and a present
    /// on the other interfaces cost next to nothing).
    pub(crate) fn wanted(renderer: &crate::Renderer) -> Option<Arc<Self>> {
        if !(omsi_cfg::flags::OMSI_PRESENT_THREAD.is_set() && crate::gl_backend()) {
            return None;
        }
        let (frames, rx) = mpsc::sync_channel::<Job>(1);
        let (done_tx, done) = mpsc::channel();
        let queue = renderer.queue.clone();
        let thread = std::thread::Builder::new()
            .name("present".into())
            .spawn(move || {
                for job in rx {
                    if !job.commands.is_empty() {
                        queue.submit(job.commands);
                    }
                    job.frame.present();
                    if done_tx.send(()).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        log::info!("the frames are submitted and presented from a thread of their own (OMSI_PRESENT_THREAD)");
        let p = Arc::new(PresentThread {
            queue: renderer.queue.clone(),
            frames: Some(frames),
            done: Mutex::new(done),
            pending: AtomicBool::new(false),
            thread: Some(thread),
        });
        *renderer.in_flight.borrow_mut() = Arc::downgrade(&p);
        Some(p)
    }

    /// Hands `commands` over to be submitted and `frame` to be presented after them.
    pub(crate) fn present(&self, commands: Vec<wgpu::CommandBuffer>, frame: wgpu::SurfaceTexture) {
        self.wait();
        if let Some(frames) = &self.frames {
            match frames.send(Job { commands, frame }) {
                Ok(()) => self.pending.store(true, Ordering::Release),
                // (the thread is gone: from here)
                Err(mpsc::SendError(job)) => {
                    if !job.commands.is_empty() {
                        self.queue.submit(job.commands);
                    }
                    job.frame.present();
                }
            }
        }
    }

    /// Returns when the last frame handed over is submitted and shown.
    pub(crate) fn wait(&self) {
        if self.pending.swap(false, Ordering::AcqRel) {
            let _ = self.done.lock().unwrap_or_else(|e| e.into_inner()).recv();
        }
    }
}

impl Drop for PresentThread {
    fn drop(&mut self) {
        self.wait();
        self.frames = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl crate::Renderer {
    /// Keep the command buffers of the window's next picture for the present thread instead
    /// of submitting them (see [`Self::take_held`]); turned off again after that `render`.
    pub fn hold_window_submit(&mut self, hold: bool) {
        self.hold_submit = hold && self.in_flight.borrow().strong_count() > 0;
    }

    /// The window picture's command buffers kept by [`Self::hold_window_submit`], to be
    /// submitted (with the frame, or right away) before anything else is.
    pub fn take_held(&mut self) -> Vec<wgpu::CommandBuffer> {
        std::mem::take(&mut self.held)
    }

    /// Returns when the frame handed to the present thread is submitted (and shown).
    pub fn settle(&self) {
        let Some(p) = self.in_flight.borrow().upgrade() else { return };
        if !p.pending.load(Ordering::Acquire) {
            return;
        }
        let start = self.profiling.then(std::time::Instant::now);
        p.wait();
        if let Some(t) = start {
            *self.stats.borrow_mut().entry("settle").or_default() += t.elapsed().as_secs_f64();
        }
    }
}

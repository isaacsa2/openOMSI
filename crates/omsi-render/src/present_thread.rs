//! The frame shown from a thread of its own (OMSI_PRESENT_THREAD=1, OpenGL and ANGLE). There
//! wgpu presents by a framebuffer blit and the swap under the GL context's lock, all in the
//! call: 9 ms of a 43 ms frame on a Radeon R7 200, 0.07 ms on Vulkan. From this thread the
//! present of one frame runs while the game's thread steps the next one (the bus, the
//! traffic, the scripts); the next frame's picture is acquired only when it is done (see
//! `SurfaceState::acquire`), so one frame is in flight as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

pub(crate) struct PresentThread {
    frames: Option<mpsc::SyncSender<wgpu::SurfaceTexture>>,
    done: Mutex<mpsc::Receiver<()>>,
    /// A frame was sent and its present not waited for.
    pending: AtomicBool,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PresentThread {
    /// The thread where it is asked for and the device is OpenGL's (a present on the other
    /// interfaces costs next to nothing).
    pub(crate) fn wanted() -> Option<Self> {
        if !(omsi_cfg::flags::OMSI_PRESENT_THREAD.is_set() && crate::gl_backend()) {
            return None;
        }
        let (frames, rx) = mpsc::sync_channel::<wgpu::SurfaceTexture>(1);
        let (done_tx, done) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("present".into())
            .spawn(move || {
                for frame in rx {
                    frame.present();
                    if done_tx.send(()).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        log::info!("the frames are presented from a thread of their own (OMSI_PRESENT_THREAD)");
        Some(PresentThread { frames: Some(frames), done: Mutex::new(done), pending: AtomicBool::new(false), thread: Some(thread) })
    }

    /// Hands `frame` over to be presented.
    pub(crate) fn present(&self, frame: wgpu::SurfaceTexture) {
        self.wait();
        if let Some(frames) = &self.frames {
            match frames.send(frame) {
                Ok(()) => self.pending.store(true, Ordering::Release),
                // (the thread is gone: shown from here)
                Err(mpsc::SendError(frame)) => frame.present(),
            }
        }
    }

    /// Returns when the last frame handed over is presented.
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

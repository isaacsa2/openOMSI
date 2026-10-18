//! Isolated first milestone of the openOMSI bgfx renderer migration.
//!
//! This exercises native bgfx initialization and a cleared view, NOT OMSI geometry.
//! It is intentionally not wired into the shipping renderer: a clear-only backend
//! must never be allowed to replace the working wgpu backend.
//!
//! The global bgfx C API is initialized once per process. Run each backend in its
//! own process; never parallelize `run` calls across threads.

pub mod frame_contract;
pub mod o3d_bridge;
pub mod gpu_mesh;
pub mod geometry_bridge;
pub mod map_bridge;
pub mod texture_bridge;
pub mod drive_bridge;

use std::{mem::MaybeUninit, time::{Duration, Instant}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Auto,
    Direct3D11,
    Direct3D12,
    Vulkan,
    OpenGl,
    Metal,
}

impl Backend {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "dx11" | "d3d11" => Some(Self::Direct3D11),
            "dx12" | "d3d12" => Some(Self::Direct3D12),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "gl" | "opengl" => Some(Self::OpenGl),
            "metal" => Some(Self::Metal),
            _ => None,
        }
    }

    fn native(self) -> bgfx_sys::bgfx_renderer_type_t {
        use bgfx_sys::*;
        match self {
            Self::Auto => BGFX_RENDERER_TYPE_COUNT,
            Self::Direct3D11 => BGFX_RENDERER_TYPE_DIRECT3D11,
            Self::Direct3D12 => BGFX_RENDERER_TYPE_DIRECT3D12,
            Self::Vulkan => BGFX_RENDERER_TYPE_VULKAN,
            Self::OpenGl => BGFX_RENDERER_TYPE_OPENGL,
            Self::Metal => BGFX_RENDERER_TYPE_METAL,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Trial {
    pub backend: Backend,
    pub width: u16,
    pub height: u16,
    pub frames: u32,
}

impl Trial {
    pub fn validate(self) -> Result<Self, &'static str> {
        if self.width == 0 || self.height == 0 {
            return Err("width and height must be nonzero");
        }
        if self.frames == 0 || self.frames > 100_000 {
            return Err("frames must be between 1 and 100000");
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub struct TrialResult {
    pub requested: Backend,
    pub native_renderer: bgfx_sys::bgfx_renderer_type_t,
    pub frames: u32,
    pub cpu_submission_elapsed: Duration,
}

impl TrialResult {
    pub fn cpu_submission_ms_per_frame(&self) -> f64 {
        self.cpu_submission_elapsed.as_secs_f64() * 1000.0 / f64::from(self.frames)
    }
}

struct Shutdown;
impl Drop for Shutdown {
    fn drop(&mut self) {
        // SAFETY: created only after a successful bgfx_init. The single-process
        // ownership contract above forbids another thread using the global API.
        unsafe { bgfx_sys::bgfx_shutdown() }
    }
}

/// Attempt native bgfx initialization and submit empty clears.
///
/// This is a headless API/driver smoke test, NOT an FPS benchmark of OMSI:
/// there are no bus meshes, WGSL shaders, textures, draw calls or swapchain
/// presentation. Headless contexts may be unsupported on some drivers.
pub fn run(trial: Trial) -> Result<TrialResult, String> {
    let trial = trial.validate().map_err(str::to_owned)?;
    let mut uninit = MaybeUninit::<bgfx_sys::bgfx_init_t>::uninit();
    // SAFETY: ctor initializes every field of the bgfx C ABI init struct.
    unsafe { bgfx_sys::bgfx_init_ctor(uninit.as_mut_ptr()) };
    let mut init = unsafe { uninit.assume_init() };
    init.type_ = trial.backend.native();
    init.resolution.width = u32::from(trial.width);
    init.resolution.height = u32::from(trial.height);
    init.resolution.reset = bgfx_sys::BGFX_RESET_NONE;
    // nwh is NULL, so this can only work when the chosen renderer/driver
    // supports a headless context. The actual game needs native winit handles.
    if !unsafe { bgfx_sys::bgfx_init(&init) } {
        return Err(format!("bgfx init failed for {:?}; this may be unsupported headless operation, not a game backend failure", trial.backend));
    }
    let _shutdown = Shutdown;
    let selected = unsafe { bgfx_sys::bgfx_get_renderer_type() };
    let start = Instant::now();
    for _ in 0..trial.frames {
        // SAFETY: global bgfx is live, view 0 is reserved for this isolated
        // test, and the dimensions were validated as u16 values.
        unsafe {
            bgfx_sys::bgfx_set_view_rect(0, 0, 0, trial.width, trial.height);
            bgfx_sys::bgfx_set_view_clear(
                0,
                (bgfx_sys::BGFX_CLEAR_COLOR | bgfx_sys::BGFX_CLEAR_DEPTH) as u16,
                0x1e2530ff,
                1.0,
                0,
            );
            bgfx_sys::bgfx_touch(0);
            bgfx_sys::bgfx_frame(false);
        }
    }
    Ok(TrialResult {
        requested: trial.backend,
        native_renderer: selected,
        frames: trial.frames,
        cpu_submission_elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backend_names_are_explicit() {
        assert_eq!(Backend::parse("dx11"), Some(Backend::Direct3D11));
        assert_eq!(Backend::parse("DX12"), Some(Backend::Direct3D12));
        assert_eq!(Backend::parse("vulkan"), Some(Backend::Vulkan));
        assert_eq!(Backend::parse("angle"), None); // ANGLE is not bgfx's native D3D11 path.
        assert_eq!(Backend::parse("nonsense"), None);
    }
    #[test]
    fn never_run_an_invalid_trial() {
        assert!(Trial { backend: Backend::Auto, width: 0, height: 100, frames: 1 }.validate().is_err());
        assert!(Trial { backend: Backend::Auto, width: 800, height: 600, frames: 0 }.validate().is_err());
        assert!(Trial { backend: Backend::Auto, width: 800, height: 600, frames: 3 }.validate().is_ok());
    }
}

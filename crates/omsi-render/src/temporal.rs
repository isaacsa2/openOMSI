//! Vendor-neutral frame contract for AMD FSR 2 and future temporal reconstruction.
//!
//! This is an input/staging contract, not the AMD FidelityFX SDK implementation.
//! The current renderer has no FSR 2 backend and must not label its spatial scaler
//! or its independent diagnostics as working FSR 2. No native SDK is loaded here.

use glam::{Mat4, Vec2, Vec3};

mod taa;
#[cfg(feature = "fsr3-wgpu")]
mod fsr3;
#[cfg(feature = "fsr3-wgpu")]
pub(crate) use fsr3::Fsr3;

impl crate::Renderer {
    pub(crate) fn fsr3_present(&self, f: &crate::passes::FrameCtx<'_>) -> Option<&wgpu::BindGroup> {
        #[cfg(feature = "fsr3-wgpu")]
        if self.fsr3_active() && f.with_overlays && f.projection.is_none() && !f.second_eye {
            return self.fsr3.as_ref().and_then(|b| b.present());
        }
        let _ = f;
        None
    }

    pub(crate) fn encode_fsr3(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        f: &crate::passes::FrameCtx<'_>,
    ) {
        #[cfg(feature = "fsr3-wgpu")]
        {
            if !self.fsr3_active() || !f.with_overlays || f.projection.is_some() || f.second_eye {
                return;
            }
            // Never dispatch on a missing, stale or differently-sized depth buffer.
            // A failed frame uses the original scaling path, not an old FSR3 image.
            let inputs = match (
                f.scene_target.as_ref(),
                self.targets.ao.as_ref().filter(|a| a.size == (f.width, f.height)),
                self.motion.as_ref().filter(|m| m.frame_ready)
                    .and_then(|m| m.target.as_ref().filter(|t| t.size == (f.width, f.height)))
            ) {
                (Some((color, _)), Some(depth), Some(motion)) => Some((
                    color.clone(), depth.depth_tex.clone(), motion.texture.clone(),
                    self.motion.as_ref().is_some_and(|m| m.frame_reset)
                )),
                _ => None,
            };
            if let Some((color, depth, motion, reset)) = inputs {
                let render = (f.width, f.height);
                let display = (f.full_w, f.target.texture().height());
                let scale = &self.scale;
                let result = self.fsr3.as_mut().expect("FSR3").encode(
                    &self.device, &self.queue, encoder,
                    &scale.upscale_layout, &scale.upscale_buf, &self.tone.post_sampler,
                    &color, &depth, &motion, render, display, f.camera,
                    f.dt, reset, self.options.fsr_sharpness,
                );
                if !result {
                    self.fsr3.as_mut().expect("FSR3").reset();
                }
            } else if let Some(fsr3) = self.fsr3.as_mut() {
                fsr3.reset();
            }
        }
        #[cfg(not(feature = "fsr3-wgpu"))]
        { let _ = (encoder, f); }
    }
}
pub(crate) use taa::Taa;

/// Only one AA/upscaling algorithm can own a frame's temporal history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemporalMethod {
    #[default]
    Native,
    Taa,
    Fsr2,
    Dlss,
}

/// Actual runtime capabilities, never inferred from vendor IDs or file names.
#[derive(Debug, Clone, Copy, Default)]
pub struct Available {
    pub taa: bool,
    pub fsr2: bool,
    pub dlss: bool,
}

/// Choose exactly one algorithm. If an unavailable temporal implementation was
/// selected, TAA is the fallback only if its backend is genuinely available.
/// FSR 1 is spatial and takes precedence over the standalone TAA path.
pub fn resolve(requested: TemporalMethod, fsr1_enabled: bool, available: Available) -> TemporalMethod {
    if fsr1_enabled {
        return TemporalMethod::Native;
    }
    match requested {
        TemporalMethod::Native => TemporalMethod::Native,
        TemporalMethod::Taa if available.taa => TemporalMethod::Taa,
        TemporalMethod::Fsr2 if available.fsr2 => TemporalMethod::Fsr2,
        TemporalMethod::Dlss if available.dlss => TemporalMethod::Dlss,
        _ if available.taa => TemporalMethod::Taa,
        _ => TemporalMethod::Native,
    }
}

/// Halton jitter in render-resolution *pixels*, not NDC or UV coordinates.
/// The phase is deterministic and uses a positive index to avoid the origin.
pub fn halton(mut index: u32, base: u32) -> f32 {
    assert!(base >= 2);
    let (mut result, mut fraction) = (0.0, 1.0 / base as f32);
    while index > 0 {
        result += (index % base) as f32 * fraction;
        index /= base;
        fraction /= base as f32;
    }
    result
}

/// A stable 8-frame baseline, separately scoped per main view. Future FSR 2
/// backends may choose a longer phase count for higher upscaling ratios.
pub fn jitter_pixels(frame: u64) -> [f32; 2] {
    let phase = (frame % 8) as u32 + 1;
    [halton(phase, 2) - 0.5, halton(phase, 3) - 0.5]
}

/// Clip-space jitter. This intentionally only computes a matrix: calling
/// this in the render pipeline without a temporal resolver would shake the image.
pub fn jittered_view_projection(unperturbed: Mat4, jitter: [f32; 2], render: (u32, u32)) -> Mat4 {
    if render.0 == 0 || render.1 == 0 || !unperturbed.is_finite() {
        return unperturbed;
    }
    let clip_x = 2.0 * jitter[0] / render.0 as f32;
    let clip_y = -2.0 * jitter[1] / render.1 as f32;
    Mat4::from_translation(Vec3::new(clip_x, clip_y, 0.0)) * unperturbed
}

/// The engine stores `previous_uv - current_uv` in RG16F. AMD expects
/// render-pixel units, so the dispatch must scale by render width/height.
pub fn motion_uv_to_pixels(uv: Vec2, render: (u32, u32)) -> Vec2 {
    uv * Vec2::new(render.0 as f32, render.1 as f32)
}

/// FSR 2 dispatch metadata. Color/depth/motion are produced by the renderer;
/// this does not claim an SDK context or infer an active backend.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    pub render_size: (u32, u32),
    pub display_size: (u32, u32),
    pub jitter_px: [f32; 2],
    /// Multiply normalized UV motion vectors by these values for FSR 2.
    pub motion_vector_scale: [f32; 2],
    pub reset: bool,
    pub reversed_depth: bool,
    pub delta_seconds: f32,
}

impl FrameInfo {
    pub fn new(render: (u32, u32), display: (u32, u32), jitter_px: [f32; 2], reset: bool, dt: f32) -> Option<Self> {
        if render.0 == 0 || render.1 == 0 || display.0 < render.0 || display.1 < render.1
            || !jitter_px.iter().all(|v| v.is_finite() && v.abs() <= 0.5)
            || !dt.is_finite() || dt < 0.0
        {
            return None;
        }
        Some(Self {
            render_size: render,
            display_size: display,
            jitter_px,
            motion_vector_scale: [render.0 as f32, render.1 as f32],
            reset,
            reversed_depth: true,
            delta_seconds: dt,
        })
    }
}

/// Actual per-frame GPU resources, borrowed only for the duration of the render.
/// Alpha-blended reactive content and pre-exposure are not currently implemented.
/// Returning None for these optional fields is honest; a future FSR 2 backend can
/// supply them while maintaining the same frame contract.
pub struct FrameTextures<'a> {
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub motion: &'a wgpu::TextureView,
    pub reactive: Option<&'a wgpu::TextureView>,
    pub transparency_composition: Option<&'a wgpu::TextureView>,
    pub exposure: Option<&'a wgpu::TextureView>,
    pub info: FrameInfo,
}

impl crate::Renderer {
    /// Borrow the real color/depth/motion textures of the completed desktop 3D frame.
    /// Not a working FSR 2 integration: these are inputs for a future official SDK
    /// dispatch. We deliberately do not claim GPU support from its availability.
    pub(crate) fn fsr2_frame_inputs<'a>(
        &'a self,
        f: &'a crate::passes::FrameCtx<'_>,
    ) -> Option<FrameTextures<'a>> {
        if self.options.fsr1 || !f.with_overlays || f.projection.is_some() || f.second_eye || self.device_lost().is_some() {
            return None;
        }
        let motion = self.motion.as_ref().filter(|m| m.frame_ready)?;
        let vector = motion.target.as_ref().filter(|t| t.size == (f.width, f.height))?;
        let depth = &self.targets.ao.as_ref()?.depth_view;
        // The current frame color must be offscreen and sampleable, not a
        // swapchain image or a stale preview. For current FSR 1-style scaling it
        // is the tone-mapped color; an official HDR path will need a separate tap.
        let color = &f.scene_target.as_ref()?.0;
        let render = (f.width, f.height);
        let display = (f.full_w, f.target.texture().height());
        // No projection jitter is applied yet. Passing invented nonzero jitter
        // to FSR 2 would be worse than rejecting the dispatch.
        let info = FrameInfo::new(render, display, [0.0, 0.0], motion.frame_reset, f.dt)?;
        Some(FrameTextures {
            color,
            depth,
            motion: &vector.view,
            reactive: None,
            transparency_composition: None,
            exposure: None,
            info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_temporal_owner_and_safe_fallback() {
        let all = Available { taa: true, fsr2: true, dlss: true };
        for requested in [TemporalMethod::Taa, TemporalMethod::Fsr2, TemporalMethod::Dlss] {
            assert_eq!(resolve(requested, true, all), TemporalMethod::Native);
            assert_eq!(resolve(requested, false, all), requested);
        }
        let without_sdk = Available { taa: true, ..Default::default() };
        assert_eq!(resolve(TemporalMethod::Fsr2, false, without_sdk), TemporalMethod::Taa);
        assert_eq!(resolve(TemporalMethod::Dlss, false, Available::default()), TemporalMethod::Native);
    }
    #[test]
    fn jitter_is_bounded_and_reproducible() {
        let first = jitter_pixels(0);
        assert_eq!(first[0], 0.0);
        assert!((first[1] + 1.0 / 6.0).abs() < 1e-6);
        for n in 0..160 {
            let v = jitter_pixels(n);
            assert_eq!(v, jitter_pixels(n % 8));
            assert!(v.iter().all(|x| *x >= -0.5 && *x <= 0.5));
        }
        let vp = Mat4::IDENTITY;
        let shifted = jittered_view_projection(vp, [0.25, -0.25], (100, 50));
        assert!((shifted.project_point3(Vec3::ZERO).x - 0.005).abs() < 1e-6);
        assert!((shifted.project_point3(Vec3::ZERO).y - 0.01).abs() < 1e-6);
    }
    #[test]
    fn ffx_dispatch_metadata_matches_engine_motion_convention() {
        let f = FrameInfo::new((1280, 720), (1920, 1080), [0.25, -0.25], true, 1.0 / 60.0).unwrap();
        assert_eq!(f.motion_vector_scale, [1280.0, 720.0]);
        assert!(f.reversed_depth && f.reset);
        assert_eq!(motion_uv_to_pixels(Vec2::new(-0.1, 0.25), f.render_size), Vec2::new(-128.0, 180.0));
        assert!(FrameInfo::new((1920, 1080), (1280, 720), [0.0; 2], false, 0.016).is_none());
        assert!(FrameInfo::new((100, 100), (100, 100), [1.0, 0.0], false, 0.016).is_none());
        assert!(FrameInfo::new((100, 100), (100, 100), [0.0; 2], false, f32::NAN).is_none());
    }
}

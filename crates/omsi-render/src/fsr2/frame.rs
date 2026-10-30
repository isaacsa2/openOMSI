//! FSR 2 in the window's frame: the jittered projection, the dispatch after the post passes,
//! and the upscaled picture handed to the present pass (the HUD drawn on it afterwards, at
//! the window's size and never jittered).

use crate::passes::{FrameCtx, PassTimers};
use crate::*;

/// FSR 2 needs the main view's readable, single-sampled prepass depth to reconstruct
/// temporal history (wet roads included: `motion_depth_ready`). Do not jitter a frame that
/// must be presented by the spatial upscaler instead.
fn fsr2_depth_usable(msaa: u32, prepass_on: bool) -> bool {
    msaa <= 1 && prepass_on
}

impl Renderer {
    /// Build FSR 2's passes and the motion vectors they read, if this device can run them
    /// (`fsr2::supported`); false when it cannot, said once in the log, or with MSAA on (the
    /// depth must be single-sampled: a restart after turning it off).
    pub(crate) fn ensure_fsr2(&mut self) -> bool {
        if self.fsr2.is_some() {
            return true;
        }
        if self.options.msaa > 1 || self.options.preview_only {
            log::info!("FSR 2: left out while MSAA is on (it needs a single-sampled depth buffer)");
            return false;
        }
        let device_features = self.device.features();
        let adapter = self.adapter.clone();
        let formats = |f: wgpu::TextureFormat| match &adapter {
            Some(a) if device_features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES) => a.get_texture_format_features(f),
            _ => f.guaranteed_format_features(device_features),
        };
        if let Err(why) = crate::fsr2::supported(&self.device, formats) {
            log::warn!("FSR 2 does not run on this device: {why}; the standard upscaler stays");
            return false;
        }
        if self.motion.is_none() {
            self.set_motion_vectors(true);
        }
        if self.motion.is_none() {
            log::warn!("FSR 2: the motion vectors are refused by this driver; the standard upscaler stays");
            return false;
        }
        let hdr = false;
        self.fsr2 = crate::gpu::optional_pipeline(&self.device, "AMD FSR 2", || crate::fsr2::Fsr2::new(&self.device, &self.queue, hdr));
        self.fsr2.is_some()
    }

    /// Whether this view's picture is FSR 2's: the window's own (not a mirror, an XR eye or
    /// a thumbnail), drawn smaller than the window, with motion vectors and a readable,
    /// single-sampled depth.
    pub(crate) fn fsr2_view(&self, f: &FrameCtx) -> bool {
        self.fsr2.is_some()
            && self.motion.is_some()
            && f.with_overlays
            && f.projection.is_none()
            && !f.second_eye
            && fsr2_depth_usable(self.options.msaa, f.prepass_on)
            && f.scene_target.is_some()
            && self.device_lost().is_none()
    }

    /// The view-projection the scene is drawn with: `vp`, offset by this frame's jitter when
    /// FSR 2 resolves it (the reference's "jitterX = 2 * jitterX / renderWidth" applied to the
    /// projection). The motion vectors are taken without it.
    pub(crate) fn raster_view_proj(&mut self, f: &FrameCtx, vp: Mat4) -> Mat4 {
        if !self.fsr2_view(f) {
            return vp;
        }
        let display = (f.full_w, f.target.texture().height());
        let render = (f.width, f.height);
        let Some(fsr2) = self.fsr2.as_mut() else { return vp };
        let j = fsr2.next_jitter(render, display);
        crate::temporal::jittered_view_projection(vp, j, render)
    }

    /// Once the frame's draws are known: when FSR 2 jittered the projection but the motion
    /// vectors cannot be worked out this frame (a presurface in the picture, no depth of the
    /// picture's size - what `fsr2_view` cannot see before the draw plan), the camera is
    /// written again without the jitter, so the standard upscaler shows a steady picture.
    pub(crate) fn unjitter_unresolvable(&mut self, f: &FrameCtx, plan: &crate::passes::DrawPlan, vp: Mat4) {
        let jittered = self.fsr2.as_ref().is_some_and(|s| s.frame_jitter.is_some());
        if !jittered || !self.fsr2_view(f) || self.motion_depth_ready(f, plan) {
            return;
        }
        let at = std::mem::offset_of!(crate::CameraUniform, view_proj) as u64;
        self.queue.write_buffer(&self.camera_buf, at, bytemuck::bytes_of(&vp.to_cols_array_2d()));
        if let Some(fsr2) = self.fsr2.as_mut() {
            fsr2.frame_jitter = None;
        }
    }

    /// Resolve the frame with FSR 2 (after the post passes, before the present). False when
    /// it did not run: the standard upscaler presents the picture then.
    pub(crate) fn encode_fsr2(&mut self, encoder: &mut wgpu::CommandEncoder, f: &FrameCtx, timers: &mut PassTimers) -> bool {
        self.fsr2_drawn.set(false);
        if !self.fsr2_view(f) {
            // A main view that temporarily cannot supply temporal inputs (for example,
            // no depth prepass) is presented with the spatial scaler. It must not reuse the
            // stale FSR 2 history when a valid depth prepass returns. Mirrors and XR
            // views must never reset the main picture's temporal history.
            if f.with_overlays && f.projection.is_none() && !f.second_eye {
                if let Some(fsr2) = self.fsr2.as_mut() {
                    fsr2.reset();
                    fsr2.frame_jitter = None;
                }
            }
            return false;
        }
        let Some(inputs) = self.fsr2_frame_inputs(f) else {
            // (no motion vectors this frame: the history cannot follow, it starts afresh)
            if let Some(fsr2) = self.fsr2.as_mut() {
                fsr2.reset();
                fsr2.frame_jitter = None;
            }
            return false;
        };
        let (color, depth, motion) = (inputs.color.clone(), inputs.depth.clone(), inputs.motion.clone());
        let info = inputs.info;
        let sharpness = (self.options.fsr2_sharpness > 0.0).then_some(self.options.fsr2_sharpness);
        let timestamps = crate::gpu::pass_timer(timers.set.as_ref(), &mut timers.timed, "FSR2").map(|t| wgpu::ComputePassTimestampWrites {
            query_set: t.query_set,
            beginning_of_pass_write_index: t.beginning_of_pass_write_index,
            end_of_pass_write_index: t.end_of_pass_write_index,
        });
        let camera = f.camera;
        let Some(fsr2) = self.fsr2.as_mut() else { return false };
        let inputs = crate::fsr2::Inputs {
            color: &color,
            depth: &depth,
            motion: &motion,
            render: info.render_size,
            display: info.display_size,
            jitter: [0.0; 2],
            near: camera.near,
            far: camera.far,
            fov_y: camera.fov_deg.to_radians(),
            frame_time_ms: info.delta_seconds * 1000.0,
            reset: info.reset,
            sharpness,
        };
        fsr2.dispatch(&self.device, &self.queue, encoder, &inputs, timestamps);
        if fsr2.present.is_none() {
            let view = fsr2.res.as_ref().expect("made by the dispatch").output.view.clone();
            let group = self.picture_group(&view);
            if let Some(fsr2) = self.fsr2.as_mut() {
                fsr2.present = Some(group);
            }
        }
        self.fsr2_drawn.set(true);
        true
    }
}

#[cfg(test)]
mod frame_fallback_tests {
    use super::fsr2_depth_usable;

    #[test]
    fn missing_depth_or_msaa_use_the_non_jittered_fallback() {
        // The same condition gates both the scene's projection jitter and dispatch.
        assert!(fsr2_depth_usable(1, true));
        assert!(!fsr2_depth_usable(1, false));
        assert!(!fsr2_depth_usable(4, true));
    }
}

//! FSR 2.2.1's host side arithmetic (ffx_fsr2.cpp of the reference, see `fsr2`): the quality
//! modes, the jitter sequence, the constant buffers and SPD's dispatch.

/// The reference's quality modes (ffxFsr2GetUpscaleRatioFromQualityMode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl Quality {
    /// Display size over render size.
    pub fn upscale_ratio(self) -> f32 {
        match self {
            Quality::Quality => 1.5,
            Quality::Balanced => 1.7,
            Quality::Performance => 2.0,
            Quality::UltraPerformance => 3.0,
        }
    }

    /// The render size for `display` (ffxFsr2GetRenderResolutionFromQualityMode: truncated).
    pub fn render_size(self, display: (u32, u32)) -> (u32, u32) {
        let r = self.upscale_ratio();
        (((display.0 as f32 / r) as u32).max(1), ((display.1 as f32 / r) as u32).max(1))
    }

    pub fn parse(s: &str) -> Option<Quality> {
        match s {
            "quality" => Some(Quality::Quality),
            "balanced" => Some(Quality::Balanced),
            "performance" => Some(Quality::Performance),
            "ultra_performance" => Some(Quality::UltraPerformance),
            _ => None,
        }
    }
}

/// ffxFsr2GetJitterPhaseCount: 8 times the square of the upscale ratio.
pub fn jitter_phase_count(render_width: u32, display_width: u32) -> i32 {
    let base_phase_count = 8.0f32;
    (base_phase_count * (display_width as f32 / render_width.max(1) as f32).powf(2.0)) as i32
}

/// The reference's halton (ffx_fsr2.cpp).
fn halton(index: i32, base: i32) -> f32 {
    let (mut f, mut result) = (1.0f32, 0.0f32);
    let mut current_index = index;
    while current_index > 0 {
        f /= base as f32;
        result += f * (current_index % base) as f32;
        current_index = (current_index as f32 / base as f32).floor() as i32;
    }
    result
}

/// ffxFsr2GetJitterOffset: the frame's sub-pixel offset in render pixels, in -0.5 .. 0.5.
pub fn jitter_offset(index: i32, phase_count: i32) -> [f32; 2] {
    let phase_count = phase_count.max(1);
    let i = index.rem_euclid(phase_count) + 1;
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

/// setupDeviceDepthToViewSpaceDepthParams for reversed, finite depth (FFX_FSR2_ENABLE_DEPTH_
/// INVERTED without FFX_FSR2_ENABLE_DEPTH_INFINITE): the shaders' view depth from a device one.
pub fn device_to_view_depth(near: f32, far: f32, fov_y: f32, render: (u32, u32)) -> [f32; 4] {
    // make sure it has no impact if near and far plane values are swapped in dispatch params
    // the flags "inverted" and "infinite" will decide what transform to use
    let (mut f_min, mut f_max) = (near.min(far), near.max(far));
    // (inverted)
    std::mem::swap(&mut f_min, &mut f_max);
    let f_q = f_max / (f_min - f_max);
    let d = -1.0f32;
    let matrix_elem_c = f_q;
    let matrix_elem_e = f_q * f_min;
    // revert x and y coords
    let aspect = render.0 as f32 / render.1 as f32;
    let cot_half_fov_y = (0.5 * fov_y).cos() / (0.5 * fov_y).sin();
    let a = cot_half_fov_y / aspect;
    let b = cot_half_fov_y;
    [d * matrix_elem_c, matrix_elem_e, 1.0 / a, 1.0 / b]
}

/// cbFSR2 (ffx_fsr2_callbacks_glsl.h; `Fsr2Constants` of fsr2/common.wgsl).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Constants {
    pub render_size: [i32; 2],
    pub max_render_size: [i32; 2],
    pub display_size: [i32; 2],
    pub input_color_resource_dimensions: [i32; 2],
    pub luma_mip_dimensions: [i32; 2],
    pub luma_mip_level_to_use: i32,
    pub frame_index: i32,
    pub device_to_view_depth: [f32; 4],
    pub jitter: [f32; 2],
    pub motion_vector_scale: [f32; 2],
    pub downscale_factor: [f32; 2],
    pub motion_vector_jitter_cancellation: [f32; 2],
    pub pre_exposure: f32,
    pub previous_frame_pre_exposure: f32,
    pub tan_half_fov: f32,
    pub jitter_sequence_length: f32,
    pub delta_time: f32,
    pub dynamic_res_change_factor: f32,
    pub view_space_to_meters_factor: f32,
    pub _pad: f32,
}

/// FFX_FSR2_SHADING_CHANGE_MIP_LEVEL: the luminance mip the locks compare.
pub const SHADING_CHANGE_MIP_LEVEL: i32 = 4;

/// The size of that mip, `lumaMipDimensions` (the reference's maximum render size over 32).
pub fn luma_mip_dimensions(max_render: (u32, u32)) -> (u32, u32) {
    let mip_div = (2u32 << SHADING_CHANGE_MIP_LEVEL) as f32;
    (((max_render.0 as f32 / mip_div) as u32).max(1), ((max_render.1 as f32 / mip_div) as u32).max(1))
}

/// The size of the luminance chain's mip 5 (its mip 0 being half the maximum render size).
pub fn luma_mip5_dimensions(max_render: (u32, u32)) -> (u32, u32) {
    (((max_render.0 / 2) >> 5).max(1), ((max_render.1 / 2) >> 5).max(1))
}

/// SpdSetup for the whole render rectangle: (dispatch groups x, y, the number of groups, mips).
pub fn spd_setup(render: (u32, u32)) -> (u32, u32, u32, u32) {
    let end_x = (render.0.max(1) - 1) / 64;
    let end_y = (render.1.max(1) - 1) / 64;
    let (gx, gy) = (end_x + 1, end_y + 1);
    let resolution = render.0.max(render.1).max(1);
    let mips = ((resolution as f32).log2().floor()).min(12.0) as u32;
    (gx, gy, gx * gy, mips)
}

/// cbSPD of the luminance pyramid pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpdConstants {
    pub mips: u32,
    pub num_work_groups: u32,
    pub work_group_offset: [u32; 2],
    pub render_size: [u32; 2],
    pub _pad: [u32; 2],
}

/// FsrRcasCon with the reference's remap of a sharpness of 0 .. 1 to stops (2 .. 0).
pub fn rcas_config(sharpness: f32) -> [u32; 4] {
    let sharpness_remapped = (-2.0 * sharpness.clamp(0.0, 1.0)) + 2.0;
    let s = (-sharpness_remapped).exp2();
    // (the second word, the 16-bit path's pair of halves, is kept as the reference packs it)
    let h = f16_bits(s);
    [s.to_bits(), h | (h << 16), 0, 0]
}

/// An f32 in 0 .. 65504 as IEEE half bits, rounded to nearest even (packHalf2x16).
fn f16_bits(v: f32) -> u32 {
    let b = v.to_bits();
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if v == 0.0 || exp <= 0 {
        return 0;
    }
    let half = ((exp as u32) << 10) | (man >> 13);
    let rest = man & 0x1fff;
    half + u32::from(rest > 0x1000 || (rest == 0x1000 && half & 1 == 1))
}

/// The state the reference keeps from one dispatch to the next.
#[derive(Debug, Clone, Default)]
pub struct History {
    pub frame_index: i32,
    pub jitter_phase_count: f32,
    pub pre_exposure: f32,
    /// The resource frame index (only its parity is used: which half of each history pair).
    pub resource_frame_index: u32,
    pub first_execution: bool,
}

/// Fill the constants of one dispatch as fsr2Dispatch does, advancing `history`. `reset`: the
/// history is not to be used (a cut, a teleport, a resize).
#[allow(clippy::too_many_arguments)]
pub fn dispatch_constants(
    history: &mut History,
    render: (u32, u32),
    max_render: (u32, u32),
    display: (u32, u32),
    input_dims: (u32, u32),
    jitter: [f32; 2],
    near: f32,
    far: f32,
    fov_y: f32,
    frame_time_ms: f32,
    reset: bool,
) -> (Constants, bool) {
    let reset_accumulation = reset || history.first_execution;
    history.first_execution = false;

    let mut c = Constants {
        jitter,
        render_size: [render.0 as i32, render.1 as i32],
        max_render_size: [max_render.0 as i32, max_render.1 as i32],
        display_size: [display.0 as i32, display.1 as i32],
        input_color_resource_dimensions: [input_dims.0 as i32, input_dims.1 as i32],
        ..Default::default()
    };

    // compute the horizontal FOV for the shader from the vertical one.
    let aspect_ratio = render.0 as f32 / render.1 as f32;
    let camera_angle_horizontal = ((fov_y / 2.0).tan() * aspect_ratio).atan() * 2.0;
    c.tan_half_fov = (camera_angle_horizontal * 0.5).tan();
    c.view_space_to_meters_factor = 1.0;

    c.device_to_view_depth = device_to_view_depth(near, far, fov_y, render);

    c.downscale_factor = [render.0 as f32 / display.0 as f32, render.1 as f32 / display.1 as f32];
    c.previous_frame_pre_exposure = history.pre_exposure;
    history.pre_exposure = 1.0;
    c.pre_exposure = history.pre_exposure;

    // motion vector data: the engine's vectors are in UV units (scale = render size), taken
    // to UV again by the reference's division
    c.motion_vector_scale = [render.0 as f32 / render.0 as f32, render.1 as f32 / render.1 as f32];

    // lock data, assuming jitter sequence length computation for now
    let jitter_phase_count = jitter_phase_count(render.0, display.0);
    if reset_accumulation || history.jitter_phase_count == 0.0 {
        history.jitter_phase_count = jitter_phase_count as f32;
    } else {
        let delta = jitter_phase_count - history.jitter_phase_count as i32;
        if delta > 0 {
            history.jitter_phase_count += 1.0;
        } else if delta < 0 {
            history.jitter_phase_count -= 1.0;
        }
    }
    c.jitter_sequence_length = history.jitter_phase_count;

    // convert delta time to seconds and clamp to [0, 1].
    c.delta_time = (frame_time_ms / 1000.0).clamp(0.0, 1.0);

    if reset_accumulation {
        history.frame_index = 0;
    } else {
        history.frame_index += 1;
    }
    c.frame_index = history.frame_index;

    // shading change usage of the SPD mip levels.
    c.luma_mip_level_to_use = SHADING_CHANGE_MIP_LEVEL;
    let (lw, lh) = luma_mip_dimensions(max_render);
    c.luma_mip_dimensions = [lw as i32, lh as i32];
    (c, reset_accumulation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quality_modes_are_the_references() {
        assert_eq!(Quality::Quality.render_size((1920, 1080)), (1280, 720));
        assert_eq!(Quality::Balanced.render_size((1920, 1080)), (1129, 635));
        assert_eq!(Quality::Performance.render_size((1920, 1080)), (960, 540));
        assert_eq!(Quality::UltraPerformance.render_size((1920, 1080)), (640, 360));
        assert_eq!(jitter_phase_count(1280, 1920), 18);
        assert_eq!(jitter_phase_count(960, 1920), 32);
    }

    #[test]
    fn the_jitter_is_the_halton_sequence_from_index_one() {
        let j = jitter_offset(0, 8);
        assert!((j[0] - 0.0).abs() < 1e-6 && (j[1] - (1.0 / 3.0 - 0.5)).abs() < 1e-6, "{j:?}");
        let j = jitter_offset(1, 8);
        assert!((j[0] - (0.25 - 0.5)).abs() < 1e-6 && (j[1] - (2.0 / 3.0 - 0.5)).abs() < 1e-6, "{j:?}");
        // repeats with the phase count, never outside half a pixel
        for i in 0..64 {
            assert_eq!(jitter_offset(i, 18), jitter_offset(i + 18, 18));
            assert!(jitter_offset(i, 18).iter().all(|v| v.abs() <= 0.5));
        }
    }

    #[test]
    fn view_depth_of_the_near_and_far_planes() {
        let k = device_to_view_depth(0.5, 3000.0, 60f32.to_radians(), (1600, 900));
        let view = |dev: f32| k[1] / (dev - k[0]);
        assert!((view(1.0) - 0.5).abs() < 1e-4, "{}", view(1.0));
        assert!((view(0.0) - 3000.0).abs() / 3000.0 < 1e-4, "{}", view(0.0));
    }

    #[test]
    fn spd_covers_the_render_size() {
        assert_eq!(spd_setup((1280, 720)), (20, 12, 240, 10));
        assert_eq!(spd_setup((64, 64)), (1, 1, 1, 6));
        assert_eq!(luma_mip_dimensions((1280, 720)), (40, 22));
        assert_eq!(luma_mip5_dimensions((1280, 720)), (20, 11));
    }

    #[test]
    fn rcas_sharpness_one_is_no_reduction() {
        assert_eq!(f32::from_bits(rcas_config(1.0)[0]), 1.0);
        assert_eq!(f32::from_bits(rcas_config(0.0)[0]), 0.25);
        // (packHalf2x16 of 1.0 and 0.25)
        assert_eq!(rcas_config(1.0)[1], 0x3c00_3c00);
        assert_eq!(rcas_config(0.0)[1], 0x3400_3400);
    }

    #[test]
    fn the_history_restarts_on_reset_and_counts_frames() {
        let mut h = History { first_execution: true, ..Default::default() };
        let args = |h: &mut History, reset| dispatch_constants(h, (1280, 720), (1280, 720), (1920, 1080), (1280, 720), [0.0; 2], 0.5, 3000.0, 1.0, 16.0, reset);
        let (c, reset) = args(&mut h, false);
        assert!(reset && c.frame_index == 0 && c.jitter_sequence_length == 18.0);
        assert_eq!(args(&mut h, false).0.frame_index, 1);
        assert_eq!(args(&mut h, false).0.frame_index, 2);
        assert_eq!(args(&mut h, true).0.frame_index, 0);
        assert_eq!(std::mem::size_of::<Constants>(), 128);
    }
}

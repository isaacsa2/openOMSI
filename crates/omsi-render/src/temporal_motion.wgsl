// Motion-vector shader for NVIDIA DLSS / DLAA staging.
// Adapted from the publicly MIT-licensed Criisko/openOMSI PR #1.
// This module is compiled only by the opt-in temporal motion pipeline.

struct MotionData {
    vp_current: mat4x4<f32>,
    vp_previous: mat4x4<f32>,
    vp_previous_here: mat4x4<f32>,
    vp_current_inverse: mat4x4<f32>,
    // xy: render resolution; z: reset history; w: previous model count.
    params: vec4<f32>,
};
@group(3) @binding(0) var<uniform> motion_data: MotionData;
@group(3) @binding(1) var<storage, read> previous_models: array<vec4<f32>>;

struct MotionVsOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) surf: f32,
    @location(5) current_clip: vec4<f32>,
    @location(6) previous_clip: vec4<f32>,
};
struct MotionFsIn {
    @builtin(position) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) surf: f32,
    @location(5) current_clip: vec4<f32>,
    @location(6) previous_clip: vec4<f32>,
};

@vertex
fn vs_temporal_motion(in: VsIn) -> MotionVsOut {
    let d = vs_depth(in);
    let entry = draw_list[in.inst];
    let m = model_matrix(entry);
    let current_world = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    // Each entry in the previous-model buffer is rebuilt against the current
    // instance's stable draw_order + material slot on the CPU. Never blindly
    // reuse an entry merely because its draw index happens to match.
    let k = entry * 4u;
    let prev_m = mat4x4<f32>(
        previous_models[k], previous_models[k + 1u],
        previous_models[k + 2u], previous_models[k + 3u]
    );
    let previous_world = prev_m * vec4<f32>(in.pos, 1.0);
    var out: MotionVsOut;
    // Same surface/depth bias and discard rules as the current depth prepass.
    out.clip = d.clip;
    out.uv = d.uv;
    out.params = d.params;
    out.surf = d.surf;
    out.current_clip = motion_data.vp_current * current_world;
    out.previous_clip = motion_data.vp_previous * previous_world;
    return out;
}

// Streamline convention for motion vectors: current pixel -> previous pixel,
// expressed in *render-resolution* pixels (x right; y down).
fn temporal_motion_pixels(now: vec4<f32>, before: vec4<f32>) -> vec4<f32> {
    if (motion_data.params.z > 0.5 || now.w <= 1e-5 || before.w <= 1e-5) {
        return vec4<f32>(0.0);
    }
    let p = (before.xy / before.w - now.xy / now.w)
        * vec2<f32>(0.5, -0.5) * motion_data.params.xy;
    return vec4<f32>(p, 0.0, 0.0);
}
@fragment
fn fs_temporal_motion(in: MotionFsIn) -> @location(0) vec2<f32> {
    return temporal_motion_pixels(in.current_clip, in.previous_clip).xy;
}
@fragment
fn fs_temporal_motion_cutout(in: MotionFsIn) -> @location(0) vec2<f32> {
    cutout_coverage(in.uv, in.params, camera.lod.x);
    return temporal_motion_pixels(in.current_clip, in.previous_clip).xy;
}
@fragment
fn fs_temporal_motion_transmap(in: MotionFsIn) -> @location(0) vec2<f32> {
    transmap_coverage(in.uv, in.params, camera.lod.x);
    return temporal_motion_pixels(in.current_clip, in.previous_clip).xy;
}

struct SkyMotion {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};
@vertex
fn vs_temporal_sky(@builtin(vertex_index) i: u32) -> SkyMotion {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: SkyMotion;
    out.ndc = xy[i];
    out.clip = vec4<f32>(xy[i], 0.0, 1.0);
    return out;
}
@fragment
fn fs_temporal_sky(in: SkyMotion) -> @location(0) vec2<f32> {
    // At reversed-Z far plane: motion of the sky is caused by camera rotation,
    // not by nearby moving geometry. This is drawn before the depth-tested mesh pass.
    let far_point = motion_data.vp_current_inverse * vec4<f32>(in.ndc, 0.0, 1.0);
    if (abs(far_point.w) < 1e-5) {
        return vec2<f32>(0.0);
    }
    let here = vec4<f32>(far_point.xyz / far_point.w, 1.0);
    return temporal_motion_pixels(
        vec4<f32>(in.ndc, 0.0, 1.0),
        motion_data.vp_previous_here * here
    ).xy;
}

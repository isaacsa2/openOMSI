// NVIDIA Streamline has produced a full-resolution linear floating point image.
// The window's sRGB conversion is handled by the render target.
@group(0) @binding(0) var dlss_result: texture_2d<f32>;
@group(0) @binding(1) var dlss_sampler: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    let p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var v: VsOut;
    v.position = vec4<f32>(p[index],0.0,1.0);
    v.uv = vec2<f32>(p[index].x * 0.5 + 0.5, 0.5 - p[index].y * 0.5);
    return v;
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let rgb = textureSampleLevel(dlss_result,dlss_sampler,in.uv,0.0).rgb;
    return vec4<f32>(max(rgb,vec3<f32>(0.0)),1.0);
}

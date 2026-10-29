// Converts the existing tone-mapped window colour into linear RGBA16F,
// which wgpu-ffx's FSR3 compute backend requires. No CPU readback.
@group(0) @binding(0) var color_source: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(color_source));
    let pixel = clamp(vec2<i32>(position.xy), vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(color_source, pixel, 0);
}

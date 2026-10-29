// Experimental standalone temporal AA, not AMD's FSR 2 upscaler.
// Reprojection uses the existing engine convention: previous UV minus current UV.
struct Params { values: vec4<f32> };
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var motion_vectors: texture_2d<f32>;
@group(0) @binding(3) var previous: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32(i32(i & 1u) * 4 - 1), f32(i32(i >> 1u) * 4 - 1), 0.0, 1.0);
}

@fragment
fn fs_taa(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let extent = vec2<i32>(textureDimensions(scene));
    let xy = clamp(vec2<i32>(frag.xy), vec2<i32>(0), extent - vec2<i32>(1));
    let current = textureLoad(scene, xy, 0);
    if (params.values.z < 0.5) {
        return current;
    }
    let uv = (vec2<f32>(xy) + vec2<f32>(0.5)) / vec2<f32>(extent);
    let velocity = textureLoad(motion_vectors, xy, 0).xy;
    let last_uv = uv + velocity;
    if (any(last_uv < vec2<f32>(0.0)) || any(last_uv > vec2<f32>(1.0))
        || any(velocity != velocity)) {
        return current;
    }
    var neighbourhood_min = vec3<f32>(1e5);
    var neighbourhood_max = vec3<f32>(-1e5);
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let pixel = clamp(xy + vec2<i32>(x, y), vec2<i32>(0), extent - vec2<i32>(1));
            let c = textureLoad(scene, pixel, 0).rgb;
            neighbourhood_min = min(neighbourhood_min, c);
            neighbourhood_max = max(neighbourhood_max, c);
        }
    }
    let old = textureSampleLevel(previous, linear_sampler, last_uv, 0.0).rgb;
    let clamped = clamp(old, neighbourhood_min, neighbourhood_max);
    let speed_px = length(velocity * vec2<f32>(extent));
    // High-velocity edges reject old color; stationary fine detail may accumulate.
    let history_weight = params.values.w * (1.0 - clamp(speed_px / 32.0, 0.0, 0.9));
    return vec4<f32>(mix(current.rgb, clamped, history_weight), current.a);
}

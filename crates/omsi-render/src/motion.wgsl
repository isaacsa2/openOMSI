// Motion vectors (OMSI_MOTION_VECTORS, see passes/motion.rs): for every pixel of the picture,
// where the same point of the world was on the screen a frame ago - the input a temporal
// upscaler (FSR 2) or anti-aliasing needs. Worked out from the depth alone: the world stands
// still and the camera moved, except inside the player's own vehicle, whose points moved with
// it (the cab moves with the camera: taken as still, its every pixel would have smeared at
// each turn). What moves on its own - the traffic, the people, the wipers - is not in it yet.
//
// The depth is reversed (near 1, far 0) and the reprojection homogeneous: a pixel of the sky,
// at depth 0, is a direction, and comes out moved by the camera's turn alone.

struct MotionParams {
    inv_view_proj: mat4x4<f32>,
    // last frame's view-projection, moved into this frame's render-origin coordinates
    prev_view_proj: mat4x4<f32>,
    // width, height, 1 / width, 1 / height
    size: vec4<f32>,
    // the player's vehicle's body: from this frame's world (render-origin relative) into its
    // model frame, and from its model frame into the world where it was a frame ago (its
    // rotation, pitch and bank on the springs included); its box half extents (w: 1 = there
    // is one) and centre, in the model frame
    to_body: mat4x4<f32>,
    from_body_prev: mat4x4<f32>,
    vehicle_box: vec4<f32>,
    vehicle_centre: vec4<f32>,
    // x: 1 when last frame's view is known (else no motion), y: the debug view's pixels of
    // motion at full colour
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> p: MotionParams;
// (the depth read as a plain float texture: GLSL cannot load from a depth texture, and wgpu
// binds a depth one so all the same)
@group(0) @binding(1) var t_depth: texture_2d<f32>;
@group(0) @binding(2) var t_motion: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

// Where a world point `w` (render-origin relative) of the player's vehicle was a frame ago:
// carried back with its body (into its model frame as it is now, out of it as it was) when it
// lies in its box (a little wider), else `w` itself.
fn with_vehicle(w: vec3<f32>) -> vec3<f32> {
    if (p.vehicle_box.w < 0.5) {
        return w;
    }
    let local = (p.to_body * vec4<f32>(w, 1.0)).xyz;
    if (any(abs(local - p.vehicle_centre.xyz) > p.vehicle_box.xyz + vec3<f32>(0.3))) {
        return w;
    }
    return (p.from_body_prev * vec4<f32>(local, 1.0)).xyz;
}

// The motion of the pixel at `px`: last frame's UV minus this frame's (FSR 2's convention).
fn motion_at(px: vec2<i32>) -> vec2<f32> {
    if (p.flags.x < 0.5) {
        return vec2<f32>(0.0);
    }
    let uv = (vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let depth = textureLoad(t_depth, px, 0).r;
    var world = p.inv_view_proj * vec4<f32>(ndc, depth, 1.0);
    if (depth > 0.0 && abs(world.w) > 1e-8) {
        world = vec4<f32>(with_vehicle(world.xyz / world.w), 1.0);
    }
    let prev = p.prev_view_proj * world;
    // (behind last frame's camera: no motion known)
    if (prev.w <= 1e-6) {
        return vec2<f32>(0.0);
    }
    let prev_ndc = prev.xy / prev.w;
    let prev_uv = vec2<f32>(prev_ndc.x * 0.5 + 0.5, 0.5 - prev_ndc.y * 0.5);
    let motion = prev_uv - uv;
    // Never write NaN, infinity or unrepresentable half-float motion into history.
    // Large camera cuts are separately handled by PreviousView's validity checks.
    if (!all(motion == motion) || any(abs(motion) > vec2<f32>(16.0))) {
        return vec2<f32>(0.0);
    }
    return motion;
}

@fragment
fn fs_motion(@builtin(position) pos: vec4<f32>) -> @location(0) vec2<f32> {
    return motion_at(vec2<i32>(pos.xy));
}

// OMSI_DEBUG_MOTION: the motion over the 3D picture (drawn onto it at its own size, before it
// is scaled up) - red across, green up and down, in pixels (`flags.y` of them at full
// colour); still pixels stay as they were.
@fragment
fn fs_debug(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let m = textureLoad(t_motion, vec2<i32>(pos.xy), 0).xy * p.size.xy;
    let c = clamp(abs(m) / max(p.flags.y, 1e-3), vec2<f32>(0.0), vec2<f32>(1.0));
    let a = clamp(max(c.x, c.y) * 4.0, 0.0, 0.8);
    return vec4<f32>(c.x, c.y, 0.0, a);
}

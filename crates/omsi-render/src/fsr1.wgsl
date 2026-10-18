// SPDX-License-Identifier: MIT
// AMD FidelityFX Super Resolution 1 (EASU + RCAS) WGSL port.
// Based on AMD's FSR 1 reference: https://github.com/GPUOpen-Effects/FidelityFX-FSR
// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
// The original MIT license notice is reproduced in fsr1-LICENSE.txt.
// The renderer draws this after tone mapping and before full-resolution overlays.
// All taps use textureLoad so older GL/GLES backends need no textureGather support.

struct Params {
    // xy: source pixels; z: RCAS strength 0..1; w: legacy FXAA selector (unused).
    src: vec4<f32>,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var s_src: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn load_texel(pixel: vec2<i32>) -> vec3<f32> {
    let extent = vec2<i32>(textureDimensions(t_src, 0));
    let bounded = clamp(pixel, vec2<i32>(0), extent - vec2<i32>(1));
    return textureLoad(t_src, bounded, 0).rgb;
}

// AMD EASU's inexpensive luma approximation.
fn luma2(col: vec3<f32>) -> f32 {
    return col.g + 0.5 * (col.r + col.b);
}

struct Edge {
    dir: vec2<f32>,
    len: f32,
};

// Accumulate a bilinear-weighted edge direction and its confidence.
fn edge_set(edge: Edge, weight: f32, a: f32, b: f32, c: f32, d: f32, e: f32) -> Edge {
    var out = edge;
    let dx = d - b;
    let dy = e - a;
    let x_span = max(max(abs(d - c), abs(c - b)), 1e-6);
    let y_span = max(max(abs(e - c), abs(c - a)), 1e-6);
    let sx = clamp(abs(dx) / x_span, 0.0, 1.0);
    let sy = clamp(abs(dy) / y_span, 0.0, 1.0);
    out.dir += vec2<f32>(dx, dy) * weight;
    out.len += (sx * sx + sy * sy) * weight;
    return out;
}

struct Accum {
    col: vec3<f32>,
    weight: f32,
};

// Reference EASU's directional, windowed-Lanczos-like 12-tap filter.
fn easu_tap(a: Accum, off: vec2<f32>, dir: vec2<f32>, anisotropy: vec2<f32>,
            lobe: f32, clip_dist: f32, col: vec3<f32>) -> Accum {
    var out = a;
    let rotated = vec2<f32>(
        off.x * dir.x + off.y * dir.y,
        -off.x * dir.y + off.y * dir.x
    ) * anisotropy;
    let dist2 = min(dot(rotated, rotated), clip_dist);
    let base = 0.4 * dist2 - 1.0;
    let window = lobe * dist2 - 1.0;
    let weight = (1.5625 * base * base - 0.5625) * window * window;
    out.col += col * weight;
    out.weight += weight;
    return out;
}

@fragment
fn fs_easu(in: VsOut) -> @location(0) vec4<f32> {
    // The fullscreen triangle interpolates the input UVs at output pixel centres.
    let pos = in.uv * params.src.xy - vec2<f32>(0.5);
    let origin = vec2<i32>(floor(pos));
    let fraction = fract(pos);

    //     b c
    //   e f g h
    //   i j k l
    //     n o
    let b = load_texel(origin + vec2<i32>( 0,-1));
    let c = load_texel(origin + vec2<i32>( 1,-1));
    let e = load_texel(origin + vec2<i32>(-1, 0));
    let f = load_texel(origin + vec2<i32>( 0, 0));
    let g = load_texel(origin + vec2<i32>( 1, 0));
    let h = load_texel(origin + vec2<i32>( 2, 0));
    let i = load_texel(origin + vec2<i32>(-1, 1));
    let j = load_texel(origin + vec2<i32>( 0, 1));
    let k = load_texel(origin + vec2<i32>( 1, 1));
    let l = load_texel(origin + vec2<i32>( 2, 1));
    let n = load_texel(origin + vec2<i32>( 0, 2));
    let o = load_texel(origin + vec2<i32>( 1, 2));

    let lb = luma2(b);
    let lc = luma2(c);
    let le = luma2(e);
    let lf = luma2(f);
    let lg = luma2(g);
    let lh = luma2(h);
    let li = luma2(i);
    let lj = luma2(j);
    let lk = luma2(k);
    let ll = luma2(l);
    let ln = luma2(n);
    let lo = luma2(o);

    var edge = Edge(vec2<f32>(0.0), 0.0);
    edge = edge_set(edge, (1.0-fraction.x)*(1.0-fraction.y), lb, le, lf, lg, lj);
    edge = edge_set(edge, fraction.x*(1.0-fraction.y), lc, lf, lg, lh, lk);
    edge = edge_set(edge, (1.0-fraction.x)*fraction.y, lf, li, lj, lk, ln);
    edge = edge_set(edge, fraction.x*fraction.y, lg, lj, lk, ll, lo);

    let dir_sq = dot(edge.dir, edge.dir);
    let dir = select(edge.dir * inverseSqrt(max(dir_sq, 1e-8)), vec2<f32>(1.0, 0.0), dir_sq < 1.0 / 32768.0);
    let shaped_len = pow(edge.len * 0.5, 2.0);
    let stretch = dot(dir, dir) / max(max(abs(dir.x), abs(dir.y)), 1e-6);
    let anisotropy = vec2<f32>(
        1.0 + (stretch - 1.0) * shaped_len,
        1.0 - 0.5 * shaped_len
    );
    let lobe = 0.5 + (0.21 - 0.5) * shaped_len;
    let clip_dist = 1.0 / lobe;

    var acc = Accum(vec3<f32>(0.0), 0.0);
    acc = easu_tap(acc, vec2<f32>( 0.0,-1.0)-fraction, dir, anisotropy, lobe, clip_dist, b);
    acc = easu_tap(acc, vec2<f32>( 1.0,-1.0)-fraction, dir, anisotropy, lobe, clip_dist, c);
    acc = easu_tap(acc, vec2<f32>(-1.0, 0.0)-fraction, dir, anisotropy, lobe, clip_dist, e);
    acc = easu_tap(acc, vec2<f32>( 0.0, 0.0)-fraction, dir, anisotropy, lobe, clip_dist, f);
    acc = easu_tap(acc, vec2<f32>( 1.0, 0.0)-fraction, dir, anisotropy, lobe, clip_dist, g);
    acc = easu_tap(acc, vec2<f32>( 2.0, 0.0)-fraction, dir, anisotropy, lobe, clip_dist, h);
    acc = easu_tap(acc, vec2<f32>(-1.0, 1.0)-fraction, dir, anisotropy, lobe, clip_dist, i);
    acc = easu_tap(acc, vec2<f32>( 0.0, 1.0)-fraction, dir, anisotropy, lobe, clip_dist, j);
    acc = easu_tap(acc, vec2<f32>( 1.0, 1.0)-fraction, dir, anisotropy, lobe, clip_dist, k);
    acc = easu_tap(acc, vec2<f32>( 2.0, 1.0)-fraction, dir, anisotropy, lobe, clip_dist, l);
    acc = easu_tap(acc, vec2<f32>( 0.0, 2.0)-fraction, dir, anisotropy, lobe, clip_dist, n);
    acc = easu_tap(acc, vec2<f32>( 1.0, 2.0)-fraction, dir, anisotropy, lobe, clip_dist, o);

    let lower = min(min(f, g), min(j, k));
    let upper = max(max(f, g), max(j, k));
    let color = clamp(acc.col / max(acc.weight, 1e-6), lower, upper);
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_rcas(in: VsOut) -> @location(0) vec4<f32> {
    // RCAS works at display resolution, using the EASU intermediate texture.
    let xy = vec2<i32>(in.clip.xy);
    let b = load_texel(xy + vec2<i32>( 0,-1));
    let d = load_texel(xy + vec2<i32>(-1, 0));
    let e = load_texel(xy);
    let f = load_texel(xy + vec2<i32>( 1, 0));
    let h = load_texel(xy + vec2<i32>( 0, 1));
    let mn4 = min(min(b, d), min(f, h));
    let mx4 = max(max(b, d), max(f, h));
    let hit_min = min(mn4, e) / max(4.0 * mx4, vec3<f32>(1e-5));
    let hit_max = (vec3<f32>(1.0) - max(mx4, e)) /
                  min(4.0 * mn4 - vec3<f32>(4.0), vec3<f32>(-1e-5));
    let lobes = max(-hit_min, hit_max);
    let lobe = max(-0.1875, min(max(lobes.r, max(lobes.g, lobes.b)), 0.0)) *
               clamp(params.src.z, 0.0, 1.0);
    let color = (e + lobe * (b + d + f + h)) / (1.0 + 4.0 * lobe);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

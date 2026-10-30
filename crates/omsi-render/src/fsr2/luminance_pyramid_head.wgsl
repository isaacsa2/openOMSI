// FSR 2.2.1 luminance pyramid, first dispatch: SpdDownsample up to its SpdExitWorkgroup (see
// luminance_pyramid.wgsl).

@group(0) @binding(2) var r_input_color_jittered: texture_2d<f32>;
@group(0) @binding(3) var s_LinearClamp: sampler;
@group(0) @binding(4) var rw_img_mip_shading_change: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var rw_img_mip_5: texture_storage_2d<r32float, write>;

fn SampleInputColor(fUV: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(r_input_color_jittered, s_LinearClamp, fUV, 0.0).rgb;
}

// SPD_SetMipmap (ffx_fsr2_compute_luminance_pyramid_pass.glsl): only the two mips FSR 2 reads.
fn SPD_SetMipmap(iPxPos: vec2<i32>, slice: u32, value: f32) {
    if (slice == u32(LumaMipLevelToUse())) {
        if (all(vec2<u32>(iPxPos) < textureDimensions(rw_img_mip_shading_change))) {
            textureStore(rw_img_mip_shading_change, iPxPos, vec4<f32>(value, 0.0, 0.0, 0.0));
        }
    } else if (slice == 5u) {
        if (all(vec2<u32>(iPxPos) < textureDimensions(rw_img_mip_5))) {
            textureStore(rw_img_mip_5, iPxPos, vec4<f32>(asHalf(value), 0.0, 0.0, 0.0));
        }
    }
}

// SpdLoadSourceImage (ffx_fsr2_compute_luminance_pyramid.h)
fn SpdLoadSourceImage(tex: vec2<f32>) -> f32 {
    var fUv = (tex + 0.5 + Jitter()) / vec2<f32>(RenderSize());
    fUv = ClampUv(fUv, RenderSize(), InputColorResourceDimensions());
    var fRgb = SampleInputColor(fUv);

    fRgb /= PreExposure();

    // compute log luma
    let fLogLuma = log(max(FSR2_EPSILON, RGBToLuma(fRgb)));

    // Make sure out of screen pixels contribute no value to the end result
    return select(0.0, fLogLuma, all(tex < vec2<f32>(RenderSize())));
}

fn SpdReduceLoadSourceImage4(i0: vec2<u32>, i1: vec2<u32>, i2: vec2<u32>, i3: vec2<u32>) -> f32 {
    let v0 = SpdLoadSourceImage(vec2<f32>(vec2<i32>(i0)));
    let v1 = SpdLoadSourceImage(vec2<f32>(vec2<i32>(i1)));
    let v2 = SpdLoadSourceImage(vec2<f32>(vec2<i32>(i2)));
    let v3 = SpdLoadSourceImage(vec2<f32>(vec2<i32>(i3)));
    return SpdReduce4(v0, v1, v2, v3);
}

fn SpdReduceLoadSourceImage(base: vec2<u32>) -> f32 {
    return SpdReduceLoadSourceImage4(base + vec2<u32>(0u, 0u), base + vec2<u32>(0u, 1u), base + vec2<u32>(1u, 0u), base + vec2<u32>(1u, 1u));
}

fn SpdDownsampleMips_0_1_LDS(x: u32, y: u32, workGroupID: vec2<u32>, localInvocationIndex: u32, mip: u32) {
    var v: array<f32, 4>;

    var tex = vec2<i32>(workGroupID.xy * 64u) + vec2<i32>(i32(x * 2u), i32(y * 2u));
    var pix = vec2<i32>(workGroupID.xy * 32u) + vec2<i32>(i32(x), i32(y));
    v[0] = SpdReduceLoadSourceImage(vec2<u32>(tex));
    SpdStore(pix, v[0], 0u);

    tex = vec2<i32>(workGroupID.xy * 64u) + vec2<i32>(i32(x * 2u + 32u), i32(y * 2u));
    pix = vec2<i32>(workGroupID.xy * 32u) + vec2<i32>(i32(x + 16u), i32(y));
    v[1] = SpdReduceLoadSourceImage(vec2<u32>(tex));
    SpdStore(pix, v[1], 0u);

    tex = vec2<i32>(workGroupID.xy * 64u) + vec2<i32>(i32(x * 2u), i32(y * 2u + 32u));
    pix = vec2<i32>(workGroupID.xy * 32u) + vec2<i32>(i32(x), i32(y + 16u));
    v[2] = SpdReduceLoadSourceImage(vec2<u32>(tex));
    SpdStore(pix, v[2], 0u);

    tex = vec2<i32>(workGroupID.xy * 64u) + vec2<i32>(i32(x * 2u + 32u), i32(y * 2u + 32u));
    pix = vec2<i32>(workGroupID.xy * 32u) + vec2<i32>(i32(x + 16u), i32(y + 16u));
    v[3] = SpdReduceLoadSourceImage(vec2<u32>(tex));
    SpdStore(pix, v[3], 0u);

    if (mip <= 1u) {
        return;
    }

    for (var i = 0u; i < 4u; i++) {
        SpdStoreIntermediate(x, y, v[i]);
        SpdWorkgroupShuffleBarrier();
        if (localInvocationIndex < 64u) {
            v[i] = SpdReduceIntermediate(vec2<u32>(x * 2u + 0u, y * 2u + 0u), vec2<u32>(x * 2u + 1u, y * 2u + 0u), vec2<u32>(x * 2u + 0u, y * 2u + 1u), vec2<u32>(x * 2u + 1u, y * 2u + 1u));
            SpdStore(vec2<i32>(workGroupID.xy * 16u) + vec2<i32>(i32(x + (i % 2u) * 8u), i32(y + (i / 2u) * 8u)), v[i], 1u);
        }
        SpdWorkgroupShuffleBarrier();
    }

    if (localInvocationIndex < 64u) {
        SpdStoreIntermediate(x + 0u, y + 0u, v[0]);
        SpdStoreIntermediate(x + 8u, y + 0u, v[1]);
        SpdStoreIntermediate(x + 0u, y + 8u, v[2]);
        SpdStoreIntermediate(x + 8u, y + 8u, v[3]);
    }
}

@compute @workgroup_size(256, 1, 1)
fn cs_luminance_pyramid(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let workGroupID = wg.xy + WorkGroupOffset();
    let mips = MipCount();
    let xy = SpdThreadXY(li);
    SpdDownsampleMips_0_1_LDS(xy.x, xy.y, workGroupID, li, mips);
    SpdDownsampleNextFour(xy.x, xy.y, workGroupID, li, 2u, mips);
}

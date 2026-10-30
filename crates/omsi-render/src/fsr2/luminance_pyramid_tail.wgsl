// FSR 2.2.1 luminance pyramid, second dispatch (one workgroup): what SpdDownsample's last
// workgroup does after SpdExitWorkgroup - mips 6 and up from mip 5, and the exposure at the
// 1 x 1 level (see luminance_pyramid.wgsl). Dispatched only when there are more than 6 mips.

@group(0) @binding(5) var r_img_mip_5: texture_2d<f32>;

// Never called here (mips 4 and 5 are the first dispatch's), but SpdStore names it.
fn SPD_SetMipmap(iPxPos: vec2<i32>, slice: u32, value: f32) {}

// SPD_LoadMipmap5: outside the image is 0, as the reference's UAV load gives.
fn SpdLoad(tex: vec2<i32>) -> f32 {
    if (any(vec2<u32>(tex) >= textureDimensions(r_img_mip_5))) {
        return 0.0;
    }
    return textureLoad(r_img_mip_5, tex, 0).x;
}

fn SpdReduceLoad4(base: vec2<u32>) -> f32 {
    let v0 = SpdLoad(vec2<i32>(base + vec2<u32>(0u, 0u)));
    let v1 = SpdLoad(vec2<i32>(base + vec2<u32>(0u, 1u)));
    let v2 = SpdLoad(vec2<i32>(base + vec2<u32>(1u, 0u)));
    let v3 = SpdLoad(vec2<i32>(base + vec2<u32>(1u, 1u)));
    return SpdReduce4(v0, v1, v2, v3);
}

fn SpdDownsampleMips_6_7(x: u32, y: u32, mips: u32) {
    var tex = vec2<u32>(x * 4u + 0u, y * 4u + 0u);
    var pix = vec2<i32>(i32(x * 2u + 0u), i32(y * 2u + 0u));
    let v0 = SpdReduceLoad4(tex);
    SpdStore(pix, v0, 6u);

    tex = vec2<u32>(x * 4u + 2u, y * 4u + 0u);
    pix = vec2<i32>(i32(x * 2u + 1u), i32(y * 2u + 0u));
    let v1 = SpdReduceLoad4(tex);
    SpdStore(pix, v1, 6u);

    tex = vec2<u32>(x * 4u + 0u, y * 4u + 2u);
    pix = vec2<i32>(i32(x * 2u + 0u), i32(y * 2u + 1u));
    let v2 = SpdReduceLoad4(tex);
    SpdStore(pix, v2, 6u);

    tex = vec2<u32>(x * 4u + 2u, y * 4u + 2u);
    pix = vec2<i32>(i32(x * 2u + 1u), i32(y * 2u + 1u));
    let v3 = SpdReduceLoad4(tex);
    SpdStore(pix, v3, 6u);

    if (mips <= 7u) {
        return;
    }
    // no barrier needed, working on values only from the same thread

    let v = SpdReduce4(v0, v1, v2, v3);
    SpdStore(vec2<i32>(i32(x), i32(y)), v, 7u);
    SpdStoreIntermediate(x, y, v);
}

@compute @workgroup_size(256, 1, 1)
fn cs_luminance_tail(@builtin(local_invocation_index) li: u32) {
    let mips = MipCount();
    let xy = SpdThreadXY(li);
    SpdDownsampleMips_6_7(xy.x, xy.y, mips);
    SpdDownsampleNextFour(xy.x, xy.y, vec2<u32>(0u, 0u), li, 8u, mips);
}

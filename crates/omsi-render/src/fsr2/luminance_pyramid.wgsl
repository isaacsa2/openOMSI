// FSR 2.2.1: the compute luminance pyramid pass (ffx_fsr2_compute_luminance_pyramid.h with
// ffx_spd.h's single pass downsampler, its LDS path: SPD_NO_WAVE_OPERATIONS).
//
// Port notes (each the same arithmetic in the same order as the reference):
// - SPD runs as two dispatches. The reference's last workgroup carries on past mip 5 once a
//   global atomic counter says the others are done, reading their mip 5 through a coherent
//   image; WGSL has no device-wide barrier that makes that read safe on every backend. So
//   `cs_luminance_pyramid` is SpdDownsample up to SpdExitWorkgroup, and `cs_luminance_tail` (one
//   workgroup, a separate dispatch) is what follows it. No atomic counter is needed.
// - The intermediate LDS keeps only the red channel: the source puts the log luma in red and 0
//   elsewhere, and only red is ever stored, so the other three arrays only ever held zeros.
// - Mip 5 (R16F in the reference) is an R32F texture written through `asHalf`; mip 4, the
//   shading change luma, an RGBA16F one. Both have the reference's mip size; stores and loads
//   outside it are dropped / read 0 explicitly, as the reference's UAV accesses are.
// - The auto exposure (RG32F) is a storage buffer of two floats.

struct SpdConstants {
    mips: u32,
    numWorkGroups: u32,
    workGroupOffset: vec2<u32>,
    renderSize: vec2<u32>,
};

@group(0) @binding(1) var<uniform> cbSPD: SpdConstants;
@group(0) @binding(6) var<storage, read_write> rw_auto_exposure: array<f32, 2>;

fn MipCount() -> u32 { return cbSPD.mips; }
fn NumWorkGroups() -> u32 { return cbSPD.numWorkGroups; }
fn WorkGroupOffset() -> vec2<u32> { return cbSPD.workGroupOffset; }

fn SPD_LoadExposureBuffer() -> vec2<f32> {
    return vec2<f32>(rw_auto_exposure[0], rw_auto_exposure[1]);
}

fn SPD_SetExposureBuffer(value: vec2<f32>) {
    rw_auto_exposure[0] = value.x;
    rw_auto_exposure[1] = value.y;
}

var<workgroup> spdIntermediateR: array<array<f32, 16>, 16>;

fn SpdLoadIntermediate(x: u32, y: u32) -> f32 {
    return spdIntermediateR[x][y];
}

fn SpdStoreIntermediate(x: u32, y: u32, value: f32) {
    spdIntermediateR[x][y] = value;
}

fn SpdReduce4(v0: f32, v1: f32, v2: f32, v3: f32) -> f32 {
    return (v0 + v1 + v2 + v3) * 0.25;
}

fn SpdReduceIntermediate(i0: vec2<u32>, i1: vec2<u32>, i2: vec2<u32>, i3: vec2<u32>) -> f32 {
    let v0 = SpdLoadIntermediate(i0.x, i0.y);
    let v1 = SpdLoadIntermediate(i1.x, i1.y);
    let v2 = SpdLoadIntermediate(i2.x, i2.y);
    let v3 = SpdLoadIntermediate(i3.x, i3.y);
    return SpdReduce4(v0, v1, v2, v3);
}

// SpdStore (ffx_fsr2_compute_luminance_pyramid.h)
fn SpdStore(pix: vec2<i32>, outValue: f32, index: u32) {
    if (index == u32(LumaMipLevelToUse()) || index == 5u) {
        SPD_SetMipmap(pix, index, outValue);
    }

    if (index == MipCount() - 1u) { // accumulate on 1x1 level
        if (all(pix == vec2<i32>(0, 0))) {
            let prev = SPD_LoadExposureBuffer().y;
            var result = outValue;

            if (prev < resetAutoExposureAverageSmoothing) { // Compare Lavg, so small or negative values
                let rate = 1.0;
                result = prev + (result - prev) * (1.0 - exp(-DeltaTime() * rate));
            }
            let spdOutput = vec2<f32>(ComputeAutoExposureFromLavg(result), result);
            SPD_SetExposureBuffer(spdOutput);
        }
    }
}

fn SpdWorkgroupShuffleBarrier() {
    workgroupBarrier();
}

// --- ffx_spd.h, SPD_NO_WAVE_OPERATIONS ------------------------------------------------------

fn SpdDownsampleMip_2(x: u32, y: u32, workGroupID: vec2<u32>, localInvocationIndex: u32, mip: u32) {
    if (localInvocationIndex < 64u) {
        let v = SpdReduceIntermediate(vec2<u32>(x * 2u + 0u, y * 2u + 0u), vec2<u32>(x * 2u + 1u, y * 2u + 0u), vec2<u32>(x * 2u + 0u, y * 2u + 1u), vec2<u32>(x * 2u + 1u, y * 2u + 1u));
        SpdStore(vec2<i32>(workGroupID.xy * 8u) + vec2<i32>(i32(x), i32(y)), v, mip);
        // store to LDS, try to reduce bank conflicts
        SpdStoreIntermediate(x * 2u + y % 2u, y * 2u, v);
    }
}

fn SpdDownsampleMip_3(x: u32, y: u32, workGroupID: vec2<u32>, localInvocationIndex: u32, mip: u32) {
    if (localInvocationIndex < 16u) {
        let v = SpdReduceIntermediate(vec2<u32>(x * 4u + 0u + 0u, y * 4u + 0u), vec2<u32>(x * 4u + 2u + 0u, y * 4u + 0u), vec2<u32>(x * 4u + 0u + 1u, y * 4u + 2u), vec2<u32>(x * 4u + 2u + 1u, y * 4u + 2u));
        SpdStore(vec2<i32>(workGroupID.xy * 4u) + vec2<i32>(i32(x), i32(y)), v, mip);
        SpdStoreIntermediate(x * 4u + y, y * 4u, v);
    }
}

fn SpdDownsampleMip_4(x: u32, y: u32, workGroupID: vec2<u32>, localInvocationIndex: u32, mip: u32) {
    if (localInvocationIndex < 4u) {
        let v = SpdReduceIntermediate(vec2<u32>(x * 8u + 0u + 0u + y * 2u, y * 8u + 0u),
                                      vec2<u32>(x * 8u + 4u + 0u + y * 2u, y * 8u + 0u),
                                      vec2<u32>(x * 8u + 0u + 1u + y * 2u, y * 8u + 4u),
                                      vec2<u32>(x * 8u + 4u + 1u + y * 2u, y * 8u + 4u));
        SpdStore(vec2<i32>(workGroupID.xy * 2u) + vec2<i32>(i32(x), i32(y)), v, mip);
        SpdStoreIntermediate(x + y * 2u, 0u, v);
    }
}

fn SpdDownsampleMip_5(workGroupID: vec2<u32>, localInvocationIndex: u32, mip: u32) {
    if (localInvocationIndex < 1u) {
        let v = SpdReduceIntermediate(vec2<u32>(0u, 0u), vec2<u32>(1u, 0u), vec2<u32>(2u, 0u), vec2<u32>(3u, 0u));
        SpdStore(vec2<i32>(workGroupID.xy), v, mip);
    }
}

fn SpdDownsampleNextFour(x: u32, y: u32, workGroupID: vec2<u32>, localInvocationIndex: u32, baseMip: u32, mips: u32) {
    if (mips <= baseMip) {
        return;
    }
    SpdWorkgroupShuffleBarrier();
    SpdDownsampleMip_2(x, y, workGroupID, localInvocationIndex, baseMip);

    if (mips <= baseMip + 1u) {
        return;
    }
    SpdWorkgroupShuffleBarrier();
    SpdDownsampleMip_3(x, y, workGroupID, localInvocationIndex, baseMip + 1u);

    if (mips <= baseMip + 2u) {
        return;
    }
    SpdWorkgroupShuffleBarrier();
    SpdDownsampleMip_4(x, y, workGroupID, localInvocationIndex, baseMip + 2u);

    if (mips <= baseMip + 3u) {
        return;
    }
    SpdWorkgroupShuffleBarrier();
    SpdDownsampleMip_5(workGroupID, localInvocationIndex, baseMip + 3u);
}

// The thread's place in the 16 x 16 block (SpdDownsample's sub_xy).
fn SpdThreadXY(localInvocationIndex: u32) -> vec2<u32> {
    let sub_xy = ffxRemapForWaveReduction(localInvocationIndex % 64u);
    let x = sub_xy.x + 8u * ((localInvocationIndex >> 6u) % 2u);
    let y = sub_xy.y + 8u * (localInvocationIndex >> 7u);
    return vec2<u32>(x, y);
}

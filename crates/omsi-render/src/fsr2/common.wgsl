// AMD FidelityFX Super Resolution 2.2.1, ported to WGSL for openOMSI (see fsr2/mod.rs).
//
// Reference: GPUOpen-Effects/FidelityFX-FSR2 at 1680d1edd5c034f88ebbbb793d8b88f8842cf804
// (tag v2.2.1), src/ffx-fsr2-api/shaders - the 32-bit path (FFX_HALF 0), low resolution motion
// vectors, motion vectors without jitter, reversed depth.
//
// This file: ffx_fsr2_common.h, the helpers of ffx_fsr2_sample.h this port uses and the
// constant buffer of ffx_fsr2_callbacks_glsl.h. Each pass's module is this file followed by the
// pass's own.
//
// Copyright (c) 2022-2023 Advanced Micro Devices, Inc. All rights reserved.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

// FFX_FSR2_OPTION_HDR_COLOR_INPUT: the colour is linear HDR (else display-referred, 0..1).
override HDR_COLOR_INPUT: bool = false;

// cbFSR2 (ffx_fsr2_callbacks_glsl.h), same order and layout.
struct Fsr2Constants {
    render_size: vec2<i32>,
    max_render_size: vec2<i32>,
    display_size: vec2<i32>,
    input_color_resource_dimensions: vec2<i32>,
    luma_mip_dimensions: vec2<i32>,
    luma_mip_level_to_use: i32,
    frame_index: i32,
    device_to_view_depth: vec4<f32>,
    jitter: vec2<f32>,
    motion_vector_scale: vec2<f32>,
    downscale_factor: vec2<f32>,
    motion_vector_jitter_cancellation: vec2<f32>,
    pre_exposure: f32,
    previous_frame_pre_exposure: f32,
    tan_half_fov: f32,
    jitter_sequence_length: f32,
    delta_time: f32,
    dynamic_res_change_factor: f32,
    view_space_to_meters_factor: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> cb: Fsr2Constants;

fn RenderSize() -> vec2<i32> { return cb.render_size; }
fn MaxRenderSize() -> vec2<i32> { return cb.max_render_size; }
fn DisplaySize() -> vec2<i32> { return cb.display_size; }
fn InputColorResourceDimensions() -> vec2<i32> { return cb.input_color_resource_dimensions; }
fn LumaMipDimensions() -> vec2<i32> { return cb.luma_mip_dimensions; }
fn LumaMipLevelToUse() -> i32 { return cb.luma_mip_level_to_use; }
fn FrameIndex() -> i32 { return cb.frame_index; }
fn DeviceToViewSpaceTransformFactors() -> vec4<f32> { return cb.device_to_view_depth; }
fn Jitter() -> vec2<f32> { return cb.jitter; }
fn MotionVectorScale() -> vec2<f32> { return cb.motion_vector_scale; }
fn DownscaleFactor() -> vec2<f32> { return cb.downscale_factor; }
fn PreExposure() -> f32 { return cb.pre_exposure; }
fn PreviousFramePreExposure() -> f32 { return cb.previous_frame_pre_exposure; }
fn JitterSequenceLength() -> f32 { return cb.jitter_sequence_length; }
fn DeltaTime() -> f32 { return cb.delta_time; }
fn ViewSpaceToMetersFactor() -> f32 { return cb.view_space_to_meters_factor; }

// --- ffx_core: the few intrinsics wrappers FSR2 uses ------------------------------------------

fn ffxSaturate(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }
fn ffxSaturate3(x: vec3<f32>) -> vec3<f32> { return clamp(x, vec3<f32>(0.0), vec3<f32>(1.0)); }
fn ffxMax3(x: f32, y: f32, z: f32) -> f32 { return max(x, max(y, z)); }
fn ffxMin3(x: f32, y: f32, z: f32) -> f32 { return min(x, min(y, z)); }

// GLSL's bitfieldInsert(src, ins, 0, bits) as ffx_core_glsl.h's bitfieldInsertMask.
fn bitfieldInsertMask(src: u32, ins: u32, bits: u32) -> u32 { return insertBits(src, ins, 0u, bits); }

// ffx_core_gpu_common.h: 64 lanes to an 8x8 block for 2D wave reductions.
fn ffxRemapForWaveReduction(a: u32) -> vec2<u32> {
    return vec2<u32>(
        bitfieldInsertMask(extractBits(a, 2u, 3u), a, 1u),
        bitfieldInsertMask(extractBits(a, 3u, 3u), extractBits(a, 1u, 2u), 2u));
}

// ffx_core_gpu_common.h: 64 lanes to an 8x8 block in quads (RCAS).
fn ffxRemapForQuad(a: u32) -> vec2<u32> {
    return vec2<u32>(extractBits(a, 1u, 3u), bitfieldInsertMask(extractBits(a, 3u, 3u), a, 1u));
}

// ffx_core_gpu_common.h: the medium precision reciprocal approximation (one Newton step).
fn ffxApproximateReciprocalMedium(value: f32) -> f32 {
    let b = bitcast<f32>(0x7ef19fffu - bitcast<u32>(value));
    return b * (-b * value + 2.0);
}

// Whether a texel lies in a resource: outside it, the reference's loads read 0 and its stores
// are dropped (D3D12 and Vulkan robust access), which WGSL leaves to the implementation.
fn InTexture(p: vec2<i32>, dims: vec2<u32>) -> bool {
    return all(vec2<u32>(p) < dims);
}

// A value as an IEEE half stores it (round to nearest even): what a 16-bit float texture keeps,
// for the resources this port holds in a wider format (no storage use of R16F/RG16F in WebGPU).
// (Rounded by hand, to nearest even: pack2x16float needs a capability older devices lack.)
fn asHalf(v: f32) -> f32 {
    let b = bitcast<u32>(v);
    let sign = b & 0x80000000u;
    let a = b & 0x7fffffffu;
    if (a >= 0x7f800000u) {
        return v; // infinity, NaN
    }
    if (a >= 0x38800000u) {
        // a normal half: 10 bits of mantissa kept, the 13 dropped rounded to nearest even
        let r = (a + 0xfffu + ((a >> 13u) & 1u)) & 0xffffe000u;
        if (r > 0x477fe000u) {
            return bitcast<f32>(sign | 0x7f800000u); // past 65504
        }
        return bitcast<f32>(sign | r);
    }
    // below 2^-14: a subnormal half, a multiple of 2^-24 (round is to nearest even)
    let q = 5.9604644775390625e-8;
    return bitcast<f32>(sign | bitcast<u32>(round(bitcast<f32>(a) / q) * q));
}

// --- ffx_fsr2_common.h ---------------------------------------------------------------------

const LOCK_LIFETIME_REMAINING: i32 = 0;
const LOCK_TEMPORAL_LUMA: i32 = 1;

const FSR2_FP16_MIN: f32 = 6.10e-05;
const FSR2_FP16_MAX: f32 = 65504.0;
const FSR2_EPSILON: f32 = 1e-03;
const FSR2_TONEMAP_EPSILON: f32 = 1.0 / FSR2_FP16_MAX;
const FSR2_FLT_MAX: f32 = 3.402823466e+38;
const FSR2_FLT_MIN: f32 = 1.175494351e-38;

// Reconstructed depth usage
const fReconstructedDepthBilinearWeightThreshold: f32 = 0.01;

// Accumulation
const fUpsampleLanczosWeightScale: f32 = 1.0 / 12.0;
const fMaxAccumulationLanczosWeight: f32 = 1.0;
const fAverageLanczosWeightPerFrame: f32 = 0.74 * fUpsampleLanczosWeightScale;
const fAccumulationMaxOnMotion: f32 = 3.0 * fUpsampleLanczosWeightScale;

// Auto exposure
const resetAutoExposureAverageSmoothing: f32 = 1e8;

struct AccumulationPassCommonParams {
    iPxHrPos: vec2<i32>,
    fHrUv: vec2<f32>,
    fLrUv_HwSampler: vec2<f32>,
    fMotionVector: vec2<f32>,
    fReprojectedHrUv: vec2<f32>,
    fHrVelocity: f32,
    fDepthClipFactor: f32,
    fDilatedReactiveFactor: f32,
    fAccumulationMask: f32,
    bIsResetFrame: bool,
    bIsExistingSample: bool,
    bIsNewSample: bool,
};

struct LockState {
    NewLock: bool,
    WasLockedPrevFrame: bool,
};

struct RectificationBox {
    boxCenter: vec3<f32>,
    boxVec: vec3<f32>,
    aabbMin: vec3<f32>,
    aabbMax: vec3<f32>,
    fBoxCenterWeight: f32,
};

fn RectificationBoxReset() -> RectificationBox {
    var b: RectificationBox;
    b.fBoxCenterWeight = 0.0;
    b.boxCenter = vec3<f32>(0.0);
    b.boxVec = vec3<f32>(0.0);
    b.aabbMin = vec3<f32>(FSR2_FLT_MAX);
    b.aabbMax = -vec3<f32>(FSR2_FLT_MAX);
    return b;
}

fn RectificationBoxAddInitialSample(b: ptr<function, RectificationBox>, colorSample: vec3<f32>, fSampleWeight: f32) {
    (*b).aabbMin = colorSample;
    (*b).aabbMax = colorSample;
    let weightedSample = colorSample * fSampleWeight;
    (*b).boxCenter = weightedSample;
    (*b).boxVec = colorSample * weightedSample;
    (*b).fBoxCenterWeight = fSampleWeight;
}

fn RectificationBoxAddSample(bInitialSample: bool, b: ptr<function, RectificationBox>, colorSample: vec3<f32>, fSampleWeight: f32) {
    if (bInitialSample) {
        RectificationBoxAddInitialSample(b, colorSample, fSampleWeight);
    } else {
        (*b).aabbMin = min((*b).aabbMin, colorSample);
        (*b).aabbMax = max((*b).aabbMax, colorSample);
        let weightedSample = colorSample * fSampleWeight;
        (*b).boxCenter += weightedSample;
        (*b).boxVec += colorSample * weightedSample;
        (*b).fBoxCenterWeight += fSampleWeight;
    }
}

fn RectificationBoxComputeVarianceBoxData(b: ptr<function, RectificationBox>) {
    (*b).fBoxCenterWeight = select(1.0, (*b).fBoxCenterWeight, abs((*b).fBoxCenterWeight) > FSR2_EPSILON);
    (*b).boxCenter /= (*b).fBoxCenterWeight;
    (*b).boxVec /= (*b).fBoxCenterWeight;
    let stdDev = sqrt(abs((*b).boxVec - (*b).boxCenter * (*b).boxCenter));
    (*b).boxVec = stdDev;
}

fn SafeRcp3(v: vec3<f32>) -> vec3<f32> {
    return select(vec3<f32>(0.0), vec3<f32>(1.0) / v, all(v != vec3<f32>(0.0)));
}

fn MinDividedByMax(v0: f32, v1: f32) -> f32 {
    let m = max(v0, v1);
    return select(0.0, min(v0, v1) / m, m != 0.0);
}

fn YCoCgToRGB(fYCoCg: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        fYCoCg.x + fYCoCg.y - fYCoCg.z,
        fYCoCg.x + fYCoCg.z,
        fYCoCg.x - fYCoCg.y - fYCoCg.z);
}

fn RGBToYCoCg(fRgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * fRgb.r + 0.5 * fRgb.g + 0.25 * fRgb.b,
        0.5 * fRgb.r - 0.5 * fRgb.b,
        -0.25 * fRgb.r + 0.5 * fRgb.g - 0.25 * fRgb.b);
}

fn RGBToLuma(fLinearRgb: vec3<f32>) -> f32 {
    return dot(fLinearRgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn RGBToPerceivedLuma(fLinearRgb: vec3<f32>) -> f32 {
    let fLuminance = RGBToLuma(fLinearRgb);
    var fPercievedLuminance = 0.0;
    if (fLuminance <= 216.0 / 24389.0) {
        fPercievedLuminance = fLuminance * (24389.0 / 27.0);
    } else {
        fPercievedLuminance = pow(fLuminance, 1.0 / 3.0) * 116.0 - 16.0;
    }
    return fPercievedLuminance * 0.01;
}

fn Tonemap(fRgb: vec3<f32>) -> vec3<f32> {
    return fRgb / vec3<f32>(max(max(0.0, fRgb.r), max(fRgb.g, fRgb.b)) + 1.0);
}

fn InverseTonemap(fRgb: vec3<f32>) -> vec3<f32> {
    return fRgb / vec3<f32>(max(FSR2_TONEMAP_EPSILON, 1.0 - max(fRgb.r, max(fRgb.g, fRgb.b))));
}

fn ClampLoad(iPxSample: vec2<i32>, iPxOffset: vec2<i32>, iTextureSize: vec2<i32>) -> vec2<i32> {
    var result = iPxSample + iPxOffset;
    result.x = select(result.x, max(result.x, 0), iPxOffset.x < 0);
    result.x = select(result.x, min(result.x, iTextureSize.x - 1), iPxOffset.x > 0);
    result.y = select(result.y, max(result.y, 0), iPxOffset.y < 0);
    result.y = select(result.y, min(result.y, iTextureSize.y - 1), iPxOffset.y > 0);
    return result;
}

fn ClampUv(fUv: vec2<f32>, iTextureSize: vec2<i32>, iResourceSize: vec2<i32>) -> vec2<f32> {
    let fSampleLocation = fUv * vec2<f32>(iTextureSize);
    let fClampedLocation = max(vec2<f32>(0.5, 0.5), min(fSampleLocation, vec2<f32>(iTextureSize) - vec2<f32>(0.5, 0.5)));
    return fClampedLocation / vec2<f32>(iResourceSize);
}

fn IsOnScreen(pos: vec2<i32>, size: vec2<i32>) -> bool {
    return all(vec2<u32>(pos) < vec2<u32>(size));
}

fn ComputeAutoExposureFromLavg(Lavg_in: f32) -> f32 {
    let Lavg = exp(Lavg_in);
    let S = 100.0; // ISO arithmetic speed
    let K = 12.5;
    let ExposureISO100 = log2((Lavg * S) / K);
    let q = 0.65;
    let Lmax = (78.0 / (q * S)) * pow(2.0, ExposureISO100);
    return 1.0 / Lmax;
}

fn ComputeHrPosFromLrPos(iPxLrPos: vec2<i32>) -> vec2<i32> {
    let fSrcJitteredPos = vec2<f32>(iPxLrPos) + 0.5 - Jitter();
    let fLrPosInHr = (fSrcJitteredPos / vec2<f32>(RenderSize())) * vec2<f32>(DisplaySize());
    return vec2<i32>(floor(fLrPosInHr));
}

fn ComputeNdc(fPxPos: vec2<f32>, iSize: vec2<i32>) -> vec2<f32> {
    return fPxPos / vec2<f32>(iSize) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
}

fn GetViewSpaceDepth(fDeviceDepth: f32) -> f32 {
    let fDeviceToViewDepth = DeviceToViewSpaceTransformFactors();
    return fDeviceToViewDepth[1] / (fDeviceDepth - fDeviceToViewDepth[0]);
}

fn GetViewSpaceDepthInMeters(fDeviceDepth: f32) -> f32 {
    return GetViewSpaceDepth(fDeviceDepth) * ViewSpaceToMetersFactor();
}

fn GetViewSpacePosition(iViewportPos: vec2<i32>, iViewportSize: vec2<i32>, fDeviceDepth: f32) -> vec3<f32> {
    let fDeviceToViewDepth = DeviceToViewSpaceTransformFactors();
    let Z = GetViewSpaceDepth(fDeviceDepth);
    let fNdcPos = ComputeNdc(vec2<f32>(iViewportPos), iViewportSize);
    let X = fDeviceToViewDepth[2] * fNdcPos.x * Z;
    let Y = fDeviceToViewDepth[3] * fNdcPos.y * Z;
    return vec3<f32>(X, Y, Z);
}

// FFX_FSR2_OPTION_INVERTED_DEPTH: the far plane is at device depth 0.
fn GetMaxDistanceInMeters() -> f32 {
    return GetViewSpaceDepth(0.0) * ViewSpaceToMetersFactor();
}

fn PrepareRgb(fRgb_in: vec3<f32>, fExposure: f32, fPreExposure: f32) -> vec3<f32> {
    var fRgb = fRgb_in / fPreExposure;
    fRgb *= fExposure;
    return clamp(fRgb, vec3<f32>(0.0), vec3<f32>(FSR2_FP16_MAX));
}

fn UnprepareRgb(fRgb_in: vec3<f32>, fExposure: f32) -> vec3<f32> {
    var fRgb = fRgb_in / fExposure;
    fRgb *= PreExposure();
    return fRgb;
}

struct BilinearSamplingData {
    iOffsets: array<vec2<i32>, 4>,
    fWeights: array<f32, 4>,
    iBasePos: vec2<i32>,
};

fn GetBilinearSamplingData(fUv: vec2<f32>, iSize: vec2<i32>) -> BilinearSamplingData {
    var data: BilinearSamplingData;
    let fPxSample = (fUv * vec2<f32>(iSize)) - vec2<f32>(0.5, 0.5);
    data.iBasePos = vec2<i32>(floor(fPxSample));
    let fPxFrac = fract(fPxSample);
    data.iOffsets[0] = vec2<i32>(0, 0);
    data.iOffsets[1] = vec2<i32>(1, 0);
    data.iOffsets[2] = vec2<i32>(0, 1);
    data.iOffsets[3] = vec2<i32>(1, 1);
    data.fWeights[0] = (1.0 - fPxFrac.x) * (1.0 - fPxFrac.y);
    data.fWeights[1] = (fPxFrac.x) * (1.0 - fPxFrac.y);
    data.fWeights[2] = (1.0 - fPxFrac.x) * (fPxFrac.y);
    data.fWeights[3] = (fPxFrac.x) * (fPxFrac.y);
    return data;
}

// --- ffx_fsr2_sample.h (the reference, sinc based Lanczos and the FSR 1 approximation) -----

fn Lanczos2NoClamp(x: f32) -> f32 {
    let PI = 3.141592653589793;
    return select((sin(PI * x) / (PI * x)) * (sin(0.5 * PI * x) / (0.5 * PI * x)), 1.0, abs(x) < FSR2_EPSILON);
}

fn Lanczos2(x_in: f32) -> f32 {
    let x = min(abs(x_in), 2.0);
    return Lanczos2NoClamp(x);
}

// FSR1 lanczos approximation. Input is x*x and must be <= 4.
fn Lanczos2ApproxSqNoClamp(x2: f32) -> f32 {
    let a = (2.0 / 5.0) * x2 - 1.0;
    let b = (1.0 / 4.0) * x2 - 1.0;
    return ((25.0 / 16.0) * a * a - (25.0 / 16.0 - 1.0)) * (b * b);
}

fn Lanczos2ApproxSq(x2_in: f32) -> f32 {
    let x2 = min(x2_in, 4.0);
    return Lanczos2ApproxSqNoClamp(x2);
}

fn Lanczos2Approx(x: f32) -> f32 {
    return Lanczos2ApproxSq(x * x);
}

fn Lanczos2_4(fColor0: vec4<f32>, fColor1: vec4<f32>, fColor2: vec4<f32>, fColor3: vec4<f32>, t: f32) -> vec4<f32> {
    let fWeight0 = Lanczos2(-1.0 - t);
    let fWeight1 = Lanczos2(-0.0 - t);
    let fWeight2 = Lanczos2(1.0 - t);
    let fWeight3 = Lanczos2(2.0 - t);
    return (fWeight0 * fColor0 + fWeight1 * fColor1 + fWeight2 * fColor2 + fWeight3 * fColor3) / (fWeight0 + fWeight1 + fWeight2 + fWeight3);
}

// FetchedBicubicSamples: 4 x 4 texels, s[col + 4 * row] for the offsets -1 .. +2.
struct BicubicSamples {
    s: array<vec4<f32>, 16>,
};

// Lanczos2(FetchedBicubicSamples, fPxFrac), with its deringing.
fn Lanczos2Bicubic(samples: BicubicSamples, fPxFrac: vec2<f32>) -> vec4<f32> {
    let fColorX0 = Lanczos2_4(samples.s[0], samples.s[1], samples.s[2], samples.s[3], fPxFrac.x);
    let fColorX1 = Lanczos2_4(samples.s[4], samples.s[5], samples.s[6], samples.s[7], fPxFrac.x);
    let fColorX2 = Lanczos2_4(samples.s[8], samples.s[9], samples.s[10], samples.s[11], fPxFrac.x);
    let fColorX3 = Lanczos2_4(samples.s[12], samples.s[13], samples.s[14], samples.s[15], fPxFrac.x);
    var fColorXY = Lanczos2_4(fColorX0, fColorX1, fColorX2, fColorX3, fPxFrac.y);

    // Deringing: fColor11, fColor21, fColor12, fColor22
    var fDeringingMin = samples.s[5];
    var fDeringingMax = samples.s[5];
    fDeringingMin = min(fDeringingMin, samples.s[6]);
    fDeringingMax = max(fDeringingMax, samples.s[6]);
    fDeringingMin = min(fDeringingMin, samples.s[9]);
    fDeringingMax = max(fDeringingMax, samples.s[9]);
    fDeringingMin = min(fDeringingMin, samples.s[10]);
    fDeringingMax = max(fDeringingMax, samples.s[10]);
    fColorXY = clamp(fColorXY, fDeringingMin, fDeringingMax);
    return fColorXY;
}

// Clamp by offset direction. Assuming iPxSample is already in range.
fn ClampCoord(iPxSample: vec2<i32>, iPxOffset: vec2<i32>, iTextureSize: vec2<i32>) -> vec2<i32> {
    return ClampLoad(iPxSample, iPxOffset, iTextureSize);
}

// The base texel and fraction of DeclareCustomTextureSample (its "clamp base coords" included).
struct CustomSamplePos {
    iPxSample: vec2<i32>,
    fPxFrac: vec2<f32>,
};

fn CustomTextureSamplePos(fUvSample: vec2<f32>, iTextureSize: vec2<i32>) -> CustomSamplePos {
    var fPxSample = (fUvSample * vec2<f32>(iTextureSize)) - vec2<f32>(0.5, 0.5);
    fPxSample.x = max(0.0, min(f32(iTextureSize.x), fPxSample.x));
    fPxSample.y = max(0.0, min(f32(iTextureSize.y), fPxSample.y));
    var p: CustomSamplePos;
    p.iPxSample = vec2<i32>(floor(fPxSample));
    p.fPxFrac = fract(fPxSample);
    return p;
}

// FSR 2.2.1: the reconstruct previous depth pass (ffx_fsr2_reconstruct_dilated_velocity_and_
// previous_depth.h, ffx_fsr2_reconstruct_previous_depth_pass.glsl).
//
// Port notes:
// - The reconstructed previous nearest depth (R32_UINT with image atomics in the reference) is
//   a storage buffer of atomic u32, one per pixel of the maximum render size, row by row:
//   image atomics are optional in WebGPU, buffer atomics are not. Same values, same max.
// - The lock input luma (R16F) is an R32F texture written through `asHalf`.
// - Loads and stores outside a resource read 0 / are dropped explicitly (`InTexture`), as the
//   reference's do under D3D12 and Vulkan robustness: threads past the render size run too.

@group(0) @binding(1) var r_input_motion_vectors: texture_2d<f32>;
@group(0) @binding(2) var r_input_depth: texture_2d<f32>;
@group(0) @binding(3) var r_input_color_jittered: texture_2d<f32>;
@group(0) @binding(4) var<storage, read> r_input_exposure: array<f32, 2>;
@group(0) @binding(5) var<storage, read_write> rw_reconstructed_previous_nearest_depth: array<atomic<u32>>;
@group(0) @binding(6) var rw_dilated_motion_vectors: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7) var rw_dilatedDepth: texture_storage_2d<r32float, write>;
@group(0) @binding(8) var rw_lock_input_luma: texture_storage_2d<r32float, write>;

fn LoadInputDepth(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_input_depth))) {
        return 0.0;
    }
    return textureLoad(r_input_depth, iPxPos, 0).r;
}

fn LoadInputColor(iPxPos: vec2<i32>) -> vec3<f32> {
    if (!InTexture(iPxPos, textureDimensions(r_input_color_jittered))) {
        return vec3<f32>(0.0);
    }
    return textureLoad(r_input_color_jittered, iPxPos, 0).rgb;
}

// FFX_FSR2_OPTION_JITTERED_MOTION_VECTORS 0
fn LoadInputMotionVector(iPxDilatedMotionVectorPos: vec2<i32>) -> vec2<f32> {
    if (!InTexture(iPxDilatedMotionVectorPos, textureDimensions(r_input_motion_vectors))) {
        return vec2<f32>(0.0);
    }
    let fSrcMotionVector = textureLoad(r_input_motion_vectors, iPxDilatedMotionVectorPos, 0).xy;
    return fSrcMotionVector * MotionVectorScale();
}

fn Exposure() -> f32 {
    var exposure = r_input_exposure[0];
    if (exposure == 0.0) {
        exposure = 1.0;
    }
    return exposure;
}

// FFX_FSR2_OPTION_INVERTED_DEPTH: the nearest is the largest.
fn StoreReconstructedDepth(iPxSample: vec2<i32>, fDepth: f32) {
    let uDepth = bitcast<u32>(fDepth);
    atomicMax(&rw_reconstructed_previous_nearest_depth[u32(iPxSample.y) * u32(MaxRenderSize().x) + u32(iPxSample.x)], uDepth);
}

fn StoreDilatedDepth(iPxPos: vec2<i32>, fDepth: f32) {
    if (!InTexture(iPxPos, textureDimensions(rw_dilatedDepth))) {
        return;
    }
    textureStore(rw_dilatedDepth, iPxPos, vec4<f32>(fDepth, 0.0, 0.0, 0.0));
}

fn StoreDilatedMotionVector(iPxPos: vec2<i32>, fMotionVector: vec2<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_dilated_motion_vectors))) {
        return;
    }
    textureStore(rw_dilated_motion_vectors, iPxPos, vec4<f32>(fMotionVector, 0.0, 0.0));
}

fn StoreLockInputLuma(iPxPos: vec2<i32>, fLuma: f32) {
    if (!InTexture(iPxPos, textureDimensions(rw_lock_input_luma))) {
        return;
    }
    textureStore(rw_lock_input_luma, iPxPos, vec4<f32>(asHalf(fLuma), 0.0, 0.0, 0.0));
}

fn ReconstructPrevDepth(iPxPos: vec2<i32>, fDepth: f32, fMotionVector_in: vec2<f32>, iPxDepthSize: vec2<i32>) {
    let fMotionVector = fMotionVector_in * f32(length(fMotionVector_in * vec2<f32>(DisplaySize())) > 0.1);

    let fUv = (vec2<f32>(iPxPos) + 0.5) / vec2<f32>(iPxDepthSize);
    let fReprojectedUv = fUv + fMotionVector;

    let bilinearInfo = GetBilinearSamplingData(fReprojectedUv, RenderSize());

    // Project current depth into previous frame locations.
    // Push to all pixels having some contribution if reprojection is using bilinear logic.
    for (var iSampleIndex = 0; iSampleIndex < 4; iSampleIndex++) {
        let iOffset = bilinearInfo.iOffsets[iSampleIndex];
        let fWeight = bilinearInfo.fWeights[iSampleIndex];

        if (fWeight > fReconstructedDepthBilinearWeightThreshold) {
            let iStorePos = bilinearInfo.iBasePos + iOffset;
            if (IsOnScreen(iStorePos, iPxDepthSize)) {
                StoreReconstructedDepth(iStorePos, fDepth);
            }
        }
    }
}

struct NearestDepth {
    fNearestDepth: f32,
    fNearestDepthCoord: vec2<i32>,
};

fn FindNearestDepth(iPxPos: vec2<i32>, iPxSize: vec2<i32>) -> NearestDepth {
    let iSampleOffsets = array<vec2<i32>, 9>(
        vec2<i32>(0, 0),
        vec2<i32>(1, 0),
        vec2<i32>(0, 1),
        vec2<i32>(0, -1),
        vec2<i32>(-1, 0),
        vec2<i32>(-1, 1),
        vec2<i32>(1, 1),
        vec2<i32>(-1, -1),
        vec2<i32>(1, -1),
    );

    // pull out the depth loads to allow SC to batch them
    var depth: array<f32, 9>;
    for (var iSampleIndex = 0; iSampleIndex < 9; iSampleIndex++) {
        let iPos = iPxPos + iSampleOffsets[iSampleIndex];
        depth[iSampleIndex] = LoadInputDepth(iPos);
    }

    // find closest depth
    var r: NearestDepth;
    r.fNearestDepthCoord = iPxPos;
    r.fNearestDepth = depth[0];
    for (var iSampleIndex = 1; iSampleIndex < 9; iSampleIndex++) {
        let iPos = iPxPos + iSampleOffsets[iSampleIndex];
        if (IsOnScreen(iPos, iPxSize)) {
            let fNdDepth = depth[iSampleIndex];
            if (fNdDepth > r.fNearestDepth) {
                r.fNearestDepthCoord = iPos;
                r.fNearestDepth = fNdDepth;
            }
        }
    }
    return r;
}

fn ComputeLockInputLuma(iPxLrPos: vec2<i32>) -> f32 {
    // We assume linear data. if non-linear input (sRGB, ...),
    // then we should convert to linear first and back to sRGB on output.
    var fRgb = max(vec3<f32>(0.0, 0.0, 0.0), LoadInputColor(iPxLrPos));

    // Use internal auto exposure for locking logic
    fRgb /= PreExposure();
    fRgb *= Exposure();

    if (HDR_COLOR_INPUT) {
        fRgb = Tonemap(fRgb);
    }

    // compute luma used to lock pixels, if used elsewhere the pow must be moved!
    return pow(RGBToPerceivedLuma(fRgb), 1.0 / 6.0);
}

fn ReconstructAndDilate(iPxLrPos: vec2<i32>) {
    let nearest = FindNearestDepth(iPxLrPos, RenderSize());
    let fDilatedDepth = nearest.fNearestDepth;

    // FFX_FSR2_OPTION_LOW_RESOLUTION_MOTION_VECTORS
    let iMotionVectorPos = nearest.fNearestDepthCoord;

    let fDilatedMotionVector = LoadInputMotionVector(iMotionVectorPos);

    StoreDilatedDepth(iPxLrPos, fDilatedDepth);
    StoreDilatedMotionVector(iPxLrPos, fDilatedMotionVector);

    ReconstructPrevDepth(iPxLrPos, fDilatedDepth, fDilatedMotionVector, RenderSize());

    let fLockInputLuma = ComputeLockInputLuma(iPxLrPos);
    StoreLockInputLuma(iPxLrPos, fLockInputLuma);
}

@compute @workgroup_size(8, 8, 1)
fn cs_reconstruct_previous_depth(@builtin(global_invocation_id) id: vec3<u32>) {
    ReconstructAndDilate(vec2<i32>(id.xy));
}

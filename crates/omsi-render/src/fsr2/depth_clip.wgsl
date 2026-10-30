// FSR 2.2.1: the depth clip pass (ffx_fsr2_depth_clip.h, ffx_fsr2_depth_clip_pass.glsl).
//
// Port notes: the reconstructed previous nearest depth is the storage buffer of
// reconstruct.wgsl; the dilated reactive masks (RG8) an RGBA8 texture, the prepared input colour
// (RGBA16F) as in the reference. Loads and stores outside a resource read 0 / are dropped
// (`InTexture`), as under D3D12 and Vulkan robustness - the 1 x 1 default reactive masks the
// reference binds when the game gives none are read that way at every pixel.

@group(0) @binding(1) var<storage, read> r_reconstructed_previous_nearest_depth: array<u32>;
@group(0) @binding(2) var r_dilated_motion_vectors: texture_2d<f32>;
@group(0) @binding(3) var r_dilatedDepth: texture_2d<f32>;
@group(0) @binding(4) var r_reactive_mask: texture_2d<f32>;
@group(0) @binding(5) var r_transparency_and_composition_mask: texture_2d<f32>;
@group(0) @binding(6) var r_previous_dilated_motion_vectors: texture_2d<f32>;
@group(0) @binding(7) var r_input_motion_vectors: texture_2d<f32>;
@group(0) @binding(8) var r_input_color_jittered: texture_2d<f32>;
@group(0) @binding(9) var r_input_depth: texture_2d<f32>;
@group(0) @binding(10) var<storage, read> r_input_exposure: array<f32, 2>;
@group(0) @binding(11) var rw_dilated_reactive_masks: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(12) var rw_prepared_input_color: texture_storage_2d<rgba16float, write>;
@group(0) @binding(13) var s_LinearClamp: sampler;

fn Exposure() -> f32 {
    var exposure = r_input_exposure[0];
    if (exposure == 0.0) {
        exposure = 1.0;
    }
    return exposure;
}

fn LoadReconstructedPrevDepth(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, vec2<u32>(MaxRenderSize()))) {
        return 0.0;
    }
    return bitcast<f32>(r_reconstructed_previous_nearest_depth[u32(iPxPos.y) * u32(MaxRenderSize().x) + u32(iPxPos.x)]);
}

fn LoadDilatedMotionVector(iPxInput: vec2<i32>) -> vec2<f32> {
    if (!InTexture(iPxInput, textureDimensions(r_dilated_motion_vectors))) {
        return vec2<f32>(0.0);
    }
    return textureLoad(r_dilated_motion_vectors, iPxInput, 0).rg;
}

fn LoadDilatedDepth(iPxInput: vec2<i32>) -> f32 {
    if (!InTexture(iPxInput, textureDimensions(r_dilatedDepth))) {
        return 0.0;
    }
    return textureLoad(r_dilatedDepth, iPxInput, 0).r;
}

fn LoadReactiveMask(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_reactive_mask))) {
        return 0.0;
    }
    return textureLoad(r_reactive_mask, iPxPos, 0).r;
}

fn LoadTransparencyAndCompositionMask(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_transparency_and_composition_mask))) {
        return 0.0;
    }
    return textureLoad(r_transparency_and_composition_mask, iPxPos, 0).r;
}

fn SamplePreviousDilatedMotionVector(fUV: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(r_previous_dilated_motion_vectors, s_LinearClamp, fUV, 0.0).xy;
}

fn LoadInputMotionVector(iPxDilatedMotionVectorPos: vec2<i32>) -> vec2<f32> {
    if (!InTexture(iPxDilatedMotionVectorPos, textureDimensions(r_input_motion_vectors))) {
        return vec2<f32>(0.0);
    }
    let fSrcMotionVector = textureLoad(r_input_motion_vectors, iPxDilatedMotionVectorPos, 0).xy;
    return fSrcMotionVector * MotionVectorScale();
}

fn LoadInputColor(iPxPos: vec2<i32>) -> vec3<f32> {
    if (!InTexture(iPxPos, textureDimensions(r_input_color_jittered))) {
        return vec3<f32>(0.0);
    }
    return textureLoad(r_input_color_jittered, iPxPos, 0).rgb;
}

fn LoadInputDepth(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_input_depth))) {
        return 0.0;
    }
    return textureLoad(r_input_depth, iPxPos, 0).r;
}

fn StoreDilatedReactiveMasks(iPxPos: vec2<i32>, fDilatedReactiveMasks: vec2<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_dilated_reactive_masks))) {
        return;
    }
    textureStore(rw_dilated_reactive_masks, iPxPos, vec4<f32>(fDilatedReactiveMasks, 0.0, 0.0));
}

fn StorePreparedInputColor(iPxPos: vec2<i32>, fTonemapped: vec4<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_prepared_input_color))) {
        return;
    }
    textureStore(rw_prepared_input_color, iPxPos, fTonemapped);
}

const DepthClipBaseScale: f32 = 4.0;

fn ComputeDepthClip(fUvSample: vec2<f32>, fCurrentDepthSample: f32) -> f32 {
    let fCurrentDepthViewSpace = GetViewSpaceDepth(fCurrentDepthSample);
    let bilinearInfo = GetBilinearSamplingData(fUvSample, RenderSize());

    var fDilatedSum = 0.0;
    var fDepth = 0.0;
    var fWeightSum = 0.0;
    for (var iSampleIndex = 0; iSampleIndex < 4; iSampleIndex++) {
        let iOffset = bilinearInfo.iOffsets[iSampleIndex];
        let iSamplePos = bilinearInfo.iBasePos + iOffset;

        if (IsOnScreen(iSamplePos, RenderSize())) {
            let fWeight = bilinearInfo.fWeights[iSampleIndex];
            if (fWeight > fReconstructedDepthBilinearWeightThreshold) {
                let fPrevDepthSample = LoadReconstructedPrevDepth(iSamplePos);
                let fPrevNearestDepthViewSpace = GetViewSpaceDepth(fPrevDepthSample);

                let fDepthDiff = fCurrentDepthViewSpace - fPrevNearestDepthViewSpace;

                if (fDepthDiff > 0.0) {
                    // FFX_FSR2_OPTION_INVERTED_DEPTH
                    let fPlaneDepth = min(fPrevDepthSample, fCurrentDepthSample);

                    let fCenter = GetViewSpacePosition(vec2<i32>(vec2<f32>(RenderSize()) * 0.5), RenderSize(), fPlaneDepth);
                    let fCorner = GetViewSpacePosition(vec2<i32>(0, 0), RenderSize(), fPlaneDepth);

                    let fHalfViewportWidth = length(vec2<f32>(RenderSize()));
                    let fDepthThreshold = max(fCurrentDepthViewSpace, fPrevNearestDepthViewSpace);

                    let Ksep = 1.37e-05;
                    let Kfov = length(fCorner) / length(fCenter);
                    let fRequiredDepthSeparation = Ksep * Kfov * fHalfViewportWidth * fDepthThreshold;

                    let fResolutionFactor = ffxSaturate(length(vec2<f32>(RenderSize())) / length(vec2<f32>(1920.0, 1080.0)));
                    let fPower = mix(1.0, 3.0, fResolutionFactor);
                    fDepth += pow(ffxSaturate(fRequiredDepthSeparation / fDepthDiff), fPower) * fWeight;
                    fWeightSum += fWeight;
                }
            }
        }
    }

    return select(0.0, ffxSaturate(1.0 - fDepth / fWeightSum), fWeightSum > 0.0);
}

fn ComputeMotionDivergence(iPxPos: vec2<i32>, iPxInputMotionVectorSize: vec2<i32>) -> f32 {
    var minconvergence = 1.0;

    let fMotionVectorNucleus = LoadInputMotionVector(iPxPos);
    let fNucleusVelocityLr = length(fMotionVectorNucleus * vec2<f32>(RenderSize()));
    var fMaxVelocityUv = length(fMotionVectorNucleus);

    let MotionVectorVelocityEpsilon = 1e-02;

    if (fNucleusVelocityLr > MotionVectorVelocityEpsilon) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let sp = ClampLoad(iPxPos, vec2<i32>(x, y), iPxInputMotionVectorSize);

                let fMotionVector = LoadInputMotionVector(sp);
                var fVelocityUv = length(fMotionVector);

                fMaxVelocityUv = max(fVelocityUv, fMaxVelocityUv);
                fVelocityUv = max(fVelocityUv, fMaxVelocityUv);
                minconvergence = min(minconvergence, dot(fMotionVector / fVelocityUv, fMotionVectorNucleus / fVelocityUv));
            }
        }
    }

    return ffxSaturate(1.0 - minconvergence) * ffxSaturate(fMaxVelocityUv / 0.01);
}

fn ComputeDepthDivergence(iPxPos: vec2<i32>) -> f32 {
    let fMaxDistInMeters = GetMaxDistanceInMeters();
    var fDepthMax = 0.0;
    var fDepthMin = fMaxDistInMeters;

    var iMaxDistFound = 0;

    for (var y = -1; y < 2; y++) {
        for (var x = -1; x < 2; x++) {
            let iOffset = vec2<i32>(x, y);
            let iSamplePos = iPxPos + iOffset;

            let fOnScreenFactor = select(0.0, 1.0, IsOnScreen(iSamplePos, RenderSize()));
            let fDepth = GetViewSpaceDepthInMeters(LoadDilatedDepth(iSamplePos)) * fOnScreenFactor;

            iMaxDistFound |= i32(fMaxDistInMeters == fDepth);

            fDepthMin = min(fDepthMin, fDepth);
            fDepthMax = max(fDepthMax, fDepth);
        }
    }

    return (1.0 - fDepthMin / fDepthMax) * select(1.0, 0.0, iMaxDistFound != 0);
}

fn ComputeTemporalMotionDivergence(iPxPos: vec2<i32>) -> f32 {
    let fUv = (vec2<f32>(iPxPos) + 0.5) / vec2<f32>(RenderSize());

    let fMotionVector = LoadDilatedMotionVector(iPxPos);
    var fReprojectedUv = fUv + fMotionVector;
    fReprojectedUv = ClampUv(fReprojectedUv, RenderSize(), MaxRenderSize());
    let fPrevMotionVector = SamplePreviousDilatedMotionVector(fReprojectedUv);

    let fPxDistance = length(fMotionVector * vec2<f32>(DisplaySize()));
    return select(0.0, mix(0.0, 1.0 - ffxSaturate(length(fPrevMotionVector) / length(fMotionVector)), ffxSaturate(pow(fPxDistance / 20.0, 3.0))), fPxDistance > 1.0);
}

fn PreProcessReactiveMasks(iPxLrPos: vec2<i32>, fMotionDivergence: f32) {
    // Compensate for bilinear sampling in accumulation pass

    let fReferenceColor = LoadInputColor(iPxLrPos).xyz;
    var fReactiveFactor = vec2<f32>(0.0, fMotionDivergence);

    var fMasksSum = 0.0;

    var fColorSamples: array<vec3<f32>, 9>;
    var fReactiveSamples: array<f32, 9>;
    var fTransparencyAndCompositionSamples: array<f32, 9>;

    for (var y = -1; y < 2; y++) {
        for (var x = -1; x < 2; x++) {
            let sampleCoord = ClampLoad(iPxLrPos, vec2<i32>(x, y), RenderSize());

            let sampleIdx = (y + 1) * 3 + x + 1;

            let fColorSample = LoadInputColor(sampleCoord).xyz;
            let fReactiveSample = LoadReactiveMask(sampleCoord);
            let fTransparencyAndCompositionSample = LoadTransparencyAndCompositionMask(sampleCoord);

            fColorSamples[sampleIdx] = fColorSample;
            fReactiveSamples[sampleIdx] = fReactiveSample;
            fTransparencyAndCompositionSamples[sampleIdx] = fTransparencyAndCompositionSample;

            fMasksSum += (fReactiveSample + fTransparencyAndCompositionSample);
        }
    }

    if (fMasksSum > 0.0) {
        for (var sampleIdx = 0; sampleIdx < 9; sampleIdx++) {
            let fColorSample = fColorSamples[sampleIdx];
            let fReactiveSample = fReactiveSamples[sampleIdx];
            let fTransparencyAndCompositionSample = fTransparencyAndCompositionSamples[sampleIdx];

            let fMaxLenSq = max(dot(fReferenceColor, fReferenceColor), dot(fColorSample, fColorSample));
            let fSimilarity = dot(fReferenceColor, fColorSample) / fMaxLenSq;

            // Increase power for non-similar samples
            let fPowerBiasMax = 6.0;
            let fSimilarityPower = 1.0 + (fPowerBiasMax - fSimilarity * fPowerBiasMax);
            let fWeightedReactiveSample = pow(fReactiveSample, fSimilarityPower);
            let fWeightedTransparencyAndCompositionSample = pow(fTransparencyAndCompositionSample, fSimilarityPower);

            fReactiveFactor = max(fReactiveFactor, vec2<f32>(fWeightedReactiveSample, fWeightedTransparencyAndCompositionSample));
        }
    }

    StoreDilatedReactiveMasks(iPxLrPos, fReactiveFactor);
}

fn ComputePreparedInputColor(iPxLrPos: vec2<i32>) -> vec3<f32> {
    // We assume linear data. if non-linear input (sRGB, ...),
    // then we should convert to linear first and back to sRGB on output.
    var fRgb = max(vec3<f32>(0.0, 0.0, 0.0), LoadInputColor(iPxLrPos));

    fRgb = PrepareRgb(fRgb, Exposure(), PreExposure());

    return RGBToYCoCg(fRgb);
}

fn EvaluateSurface(iPxPos: vec2<i32>, fMotionVector: vec2<f32>) -> f32 {
    let d0 = GetViewSpaceDepth(LoadReconstructedPrevDepth(iPxPos + vec2<i32>(0, -1)));
    let d1 = GetViewSpaceDepth(LoadReconstructedPrevDepth(iPxPos + vec2<i32>(0, 0)));
    let d2 = GetViewSpaceDepth(LoadReconstructedPrevDepth(iPxPos + vec2<i32>(0, 1)));

    return 1.0 - f32(((d0 - d1) > (d1 * 0.01)) && ((d1 - d2) > (d2 * 0.01)));
}

fn DepthClip(iPxPos: vec2<i32>) {
    let fDepthUv = (vec2<f32>(iPxPos) + 0.5) / vec2<f32>(RenderSize());
    var fMotionVector = LoadDilatedMotionVector(iPxPos);

    // Discard tiny mvs
    fMotionVector *= f32(length(fMotionVector * vec2<f32>(DisplaySize())) > 0.01);

    let fDilatedUv = fDepthUv + fMotionVector;
    let fDilatedDepth = LoadDilatedDepth(iPxPos);
    let fCurrentDepthViewSpace = GetViewSpaceDepth(LoadInputDepth(iPxPos));

    // Compute prepared input color and depth clip
    let fDepthClip = ComputeDepthClip(fDilatedUv, fDilatedDepth) * EvaluateSurface(iPxPos, fMotionVector);
    let fPreparedYCoCg = ComputePreparedInputColor(iPxPos);
    StorePreparedInputColor(iPxPos, vec4<f32>(fPreparedYCoCg, fDepthClip));

    // Compute dilated reactive mask (FFX_FSR2_OPTION_LOW_RESOLUTION_MOTION_VECTORS)
    let iSamplePos = iPxPos;

    let fMotionDivergence = ComputeMotionDivergence(iSamplePos, RenderSize());
    let fTemporalMotionDifference = ffxSaturate(ComputeTemporalMotionDivergence(iPxPos) - ComputeDepthDivergence(iPxPos));

    PreProcessReactiveMasks(iPxPos, max(fTemporalMotionDifference, fMotionDivergence));
}

@compute @workgroup_size(8, 8, 1)
fn cs_depth_clip(@builtin(global_invocation_id) id: vec3<u32>) {
    DepthClip(vec2<i32>(id.xy));
}

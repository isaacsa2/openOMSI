// FSR 2.2.1: the accumulate pass (ffx_fsr2_accumulate.h, ffx_fsr2_reproject.h,
// ffx_fsr2_upsample.h, ffx_fsr2_postprocess_lock_status.h, ffx_fsr2_accumulate_pass.glsl).
//
// Port notes:
// - History reprojection uses the reference's sinc Lanczos (FFX_FSR2_OPTION_REPROJECT_USE_
//   LANCZOS_TYPE 0, what the reference picks on wave32 hardware); the upsampling kernel its
//   approximation (FFX_FSR2_OPTION_UPSAMPLE_USE_LANCZOS_TYPE 2), as in every build of it.
// - The lock status (RG16F) is an RGBA16F texture, the luma history RGBA8 as in the reference.
// - The new locks are only read here (see lock.wgsl: the host clears them before the lock
//   pass, where the reference clears each texel at the end of this one).
// - `cs_accumulate` writes the upscaled output (FFX_FSR2_OPTION_APPLY_SHARPENING 0),
//   `cs_accumulate_sharpen` leaves it to the RCAS pass.

@group(0) @binding(1) var<storage, read> r_input_exposure: array<f32, 2>;
@group(0) @binding(2) var r_dilated_reactive_masks: texture_2d<f32>;
@group(0) @binding(3) var r_dilated_motion_vectors: texture_2d<f32>;
@group(0) @binding(4) var r_internal_upscaled_color: texture_2d<f32>;
@group(0) @binding(5) var r_lock_status: texture_2d<f32>;
@group(0) @binding(6) var r_prepared_input_color: texture_2d<f32>;
@group(0) @binding(7) var r_luma_history: texture_2d<f32>;
@group(0) @binding(8) var r_img_mip_shading_change: texture_2d<f32>;
@group(0) @binding(9) var r_new_locks: texture_2d<f32>;
@group(0) @binding(10) var s_LinearClamp: sampler;
@group(0) @binding(11) var rw_internal_upscaled_color: texture_storage_2d<rgba16float, write>;
@group(0) @binding(12) var rw_lock_status: texture_storage_2d<rgba16float, write>;
@group(0) @binding(13) var rw_luma_history: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(14) var rw_upscaled_output: texture_storage_2d<rgba16float, write>;

// --- resource access (ffx_fsr2_callbacks_glsl.h) ------------------------------------------

fn Exposure() -> f32 {
    var exposure = r_input_exposure[0];
    if (exposure == 0.0) {
        exposure = 1.0;
    }
    return exposure;
}

fn SampleMipLuma(fUV: vec2<f32>) -> f32 {
    // (the shading change mip, LumaMipLevelToUse() of the reference's luminance chain, is a
    // texture of its own here: its level 0 at the same size)
    return textureSampleLevel(r_img_mip_shading_change, s_LinearClamp, fUV, 0.0).r;
}

fn LoadPreparedInputColor(iPxPos: vec2<i32>) -> vec3<f32> {
    if (!InTexture(iPxPos, textureDimensions(r_prepared_input_color))) {
        return vec3<f32>(0.0);
    }
    return textureLoad(r_prepared_input_color, iPxPos, 0).xyz;
}

fn SampleDepthClip(fUV: vec2<f32>) -> f32 {
    return textureSampleLevel(r_prepared_input_color, s_LinearClamp, fUV, 0.0).w;
}

fn LoadHistory(iPxHistory: vec2<i32>) -> vec4<f32> {
    if (!InTexture(iPxHistory, textureDimensions(r_internal_upscaled_color))) {
        return vec4<f32>(0.0);
    }
    return textureLoad(r_internal_upscaled_color, iPxHistory, 0);
}

fn StoreLumaHistory(iPxPos: vec2<i32>, fLumaHistory: vec4<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_luma_history))) {
        return;
    }
    textureStore(rw_luma_history, iPxPos, fLumaHistory);
}

fn SampleLumaHistory(fUV: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(r_luma_history, s_LinearClamp, fUV, 0.0);
}

fn StoreInternalColorAndWeight(iPxPos: vec2<i32>, fColorAndWeight: vec4<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_internal_upscaled_color))) {
        return;
    }
    textureStore(rw_internal_upscaled_color, iPxPos, fColorAndWeight);
}

fn StoreLockStatus(iPxPos: vec2<i32>, fLockstatus: vec2<f32>) {
    if (!InTexture(iPxPos, textureDimensions(rw_lock_status))) {
        return;
    }
    textureStore(rw_lock_status, iPxPos, vec4<f32>(fLockstatus, 0.0, 0.0));
}

fn SampleLockStatus(fUV: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(r_lock_status, s_LinearClamp, fUV, 0.0).rg;
}

fn LoadRwNewLocks(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_new_locks))) {
        return 0.0;
    }
    return textureLoad(r_new_locks, iPxPos, 0).r;
}

fn LoadDilatedMotionVector(iPxInput: vec2<i32>) -> vec2<f32> {
    if (!InTexture(iPxInput, textureDimensions(r_dilated_motion_vectors))) {
        return vec2<f32>(0.0);
    }
    return textureLoad(r_dilated_motion_vectors, iPxInput, 0).rg;
}

fn SampleDilatedReactiveMasks(fUV: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(r_dilated_reactive_masks, s_LinearClamp, fUV, 0.0).rg;
}

// --- ffx_fsr2_reproject.h ----------------------------------------------------------------

// DeclareCustomFetchBicubicSamples(FetchHistorySamples, WrapHistory)
fn FetchHistorySamples(iPxSample: vec2<i32>, iTextureSize: vec2<i32>) -> BicubicSamples {
    var samples: BicubicSamples;
    for (var row = 0; row < 4; row++) {
        for (var col = 0; col < 4; col++) {
            samples.s[col + 4 * row] = LoadHistory(ClampCoord(iPxSample, vec2<i32>(col - 1, row - 1), iTextureSize));
        }
    }
    return samples;
}

// DeclareCustomTextureSample(HistorySample, Lanczos2, FetchHistorySamples)
fn HistorySample(fUvSample: vec2<f32>, iTextureSize: vec2<i32>) -> vec4<f32> {
    let p = CustomTextureSamplePos(fUvSample, iTextureSize);
    return Lanczos2Bicubic(FetchHistorySamples(p.iPxSample, iTextureSize), p.fPxFrac);
}

// FFX_FSR2_OPTION_LOW_RESOLUTION_MOTION_VECTORS
fn GetMotionVector(iPxHrPos: vec2<i32>, fHrUv: vec2<f32>) -> vec2<f32> {
    return LoadDilatedMotionVector(vec2<i32>(fHrUv * vec2<f32>(RenderSize())));
}

fn IsUvInside(fUv: vec2<f32>) -> bool {
    return (fUv.x >= 0.0 && fUv.x <= 1.0) && (fUv.y >= 0.0 && fUv.y <= 1.0);
}

struct ReprojectedHistory {
    fHistoryColor: vec3<f32>,
    fTemporalReactiveFactor: f32,
    bInMotionLastFrame: bool,
};

fn ReprojectHistoryColor(params: AccumulationPassCommonParams) -> ReprojectedHistory {
    let fHistory = HistorySample(params.fReprojectedHrUv, DisplaySize());

    var r: ReprojectedHistory;
    r.fHistoryColor = PrepareRgb(fHistory.rgb, Exposure(), PreviousFramePreExposure());

    r.fHistoryColor = RGBToYCoCg(r.fHistoryColor);

    // Compute temporal reactivity info
    r.fTemporalReactiveFactor = ffxSaturate(abs(fHistory.w));
    r.bInMotionLastFrame = (fHistory.w < 0.0);
    return r;
}

fn ReprojectHistoryLockStatus(params: AccumulationPassCommonParams, fReprojectedLockStatus: ptr<function, vec2<f32>>) -> LockState {
    var state = LockState(false, false);
    let fNewLockIntensity = LoadRwNewLocks(params.iPxHrPos);
    state.NewLock = fNewLockIntensity > (127.0 / 255.0);

    *fReprojectedLockStatus = SampleLockStatus(params.fReprojectedHrUv);

    if ((*fReprojectedLockStatus)[LOCK_LIFETIME_REMAINING] != 0.0) {
        state.WasLockedPrevFrame = true;
    }

    return state;
}

// --- ffx_fsr2_postprocess_lock_status.h --------------------------------------------------

fn GetShadingChangeLuma(iPxHrPos: vec2<i32>, fUvCoord_in: vec2<f32>) -> f32 {
    let fDiv = f32(2 << u32(LumaMipLevelToUse()));
    let iMipRenderSize = vec2<i32>(vec2<f32>(RenderSize()) / fDiv);

    let fUvCoord = ClampUv(fUvCoord_in, iMipRenderSize, LumaMipDimensions());
    var fShadingChangeLuma = Exposure() * exp(SampleMipLuma(fUvCoord));

    fShadingChangeLuma = pow(fShadingChangeLuma, 1.0 / 6.0);

    return fShadingChangeLuma;
}

struct LockUpdate {
    fLockContributionThisFrame: f32,
    fLuminanceDiff: f32,
};

fn UpdateLockStatus(params: AccumulationPassCommonParams, fReactiveFactor: ptr<function, f32>, state: LockState, fLockStatus: ptr<function, vec2<f32>>) -> LockUpdate {
    let fShadingChangeLuma = GetShadingChangeLuma(params.iPxHrPos, params.fHrUv);

    // init temporal shading change factor, init to -1 or so in reproject to know if "true new"?
    (*fLockStatus)[LOCK_TEMPORAL_LUMA] = select((*fLockStatus)[LOCK_TEMPORAL_LUMA], fShadingChangeLuma, (*fLockStatus)[LOCK_TEMPORAL_LUMA] == 0.0);

    let fPreviousShadingChangeLuma = (*fLockStatus)[LOCK_TEMPORAL_LUMA];

    var u: LockUpdate;
    u.fLuminanceDiff = 1.0 - MinDividedByMax(fPreviousShadingChangeLuma, fShadingChangeLuma);

    if (state.NewLock) {
        (*fLockStatus)[LOCK_TEMPORAL_LUMA] = fShadingChangeLuma;

        (*fLockStatus)[LOCK_LIFETIME_REMAINING] = select(1.0, 2.0, (*fLockStatus)[LOCK_LIFETIME_REMAINING] != 0.0);
    } else if ((*fLockStatus)[LOCK_LIFETIME_REMAINING] <= 1.0) {
        (*fLockStatus)[LOCK_TEMPORAL_LUMA] = mix((*fLockStatus)[LOCK_TEMPORAL_LUMA], fShadingChangeLuma, 0.5);
    } else {
        if (u.fLuminanceDiff > 0.1) {
            (*fLockStatus)[LOCK_LIFETIME_REMAINING] = 0.0; // KillLock
        }
    }

    *fReactiveFactor = max(*fReactiveFactor, ffxSaturate((u.fLuminanceDiff - 0.1) * 10.0));
    (*fLockStatus)[LOCK_LIFETIME_REMAINING] *= (1.0 - *fReactiveFactor);

    (*fLockStatus)[LOCK_LIFETIME_REMAINING] *= ffxSaturate(1.0 - params.fAccumulationMask);
    (*fLockStatus)[LOCK_LIFETIME_REMAINING] *= f32(params.fDepthClipFactor < 0.1);

    // Compute this frame lock contribution
    let fLifetimeContribution = ffxSaturate((*fLockStatus)[LOCK_LIFETIME_REMAINING] - 1.0);
    let fShadingChangeContribution = ffxSaturate(MinDividedByMax((*fLockStatus)[LOCK_TEMPORAL_LUMA], fShadingChangeLuma));

    u.fLockContributionThisFrame = ffxSaturate(ffxSaturate(fLifetimeContribution * 4.0) * fShadingChangeContribution);
    return u;
}

// --- ffx_fsr2_upsample.h -----------------------------------------------------------------

const iLanczos2SampleCount: u32 = 16u;

fn Deringing(clippingBox: RectificationBox, fColor: vec3<f32>) -> vec3<f32> {
    return clamp(fColor, clippingBox.aabbMin, clippingBox.aabbMax);
}

// FFX_FSR2_OPTION_UPSAMPLE_USE_LANCZOS_TYPE 2 (approximate)
fn GetUpsampleLanczosWeight(fSrcSampleOffset: vec2<f32>, fKernelWeight: f32) -> f32 {
    let fSrcSampleOffsetBiased = fSrcSampleOffset * vec2<f32>(fKernelWeight);
    return Lanczos2ApproxSq(dot(fSrcSampleOffsetBiased, fSrcSampleOffsetBiased));
}

fn ComputeMaxKernelWeight() -> f32 {
    let fKernelSizeBias = 1.0;
    let fKernelWeight = 1.0 + (vec2<f32>(1.0) / DownscaleFactor() - 1.0).x * fKernelSizeBias;
    return min(1.99, fKernelWeight);
}

fn ComputeUpsampledColorAndWeight(params: AccumulationPassCommonParams, clippingBox: ptr<function, RectificationBox>, fReactiveFactor: f32) -> vec4<f32> {
    // We compute a sliced lanczos filter with 2 lobes (other slices are accumulated temporaly)
    let fDstOutputPos = vec2<f32>(params.iPxHrPos) + vec2<f32>(0.5); // Destination resolution output pixel center position
    let fSrcOutputPos = fDstOutputPos * DownscaleFactor();           // Source resolution output pixel center position
    let iSrcInputPos = vec2<i32>(floor(fSrcOutputPos));

    var fSamples: array<vec3<f32>, 16>;

    let fSrcUnjitteredPos = (vec2<f32>(iSrcInputPos) + vec2<f32>(0.5, 0.5)) - Jitter(); // This is the un-jittered position of the sample at offset 0,0

    var offsetTL: vec2<i32>;
    offsetTL.x = select(-1, -2, fSrcUnjitteredPos.x > fSrcOutputPos.x);
    offsetTL.y = select(-1, -2, fSrcUnjitteredPos.y > fSrcOutputPos.y);

    // Load samples
    // If fSrcUnjitteredPos.y > fSrcOutputPos.y, indicates offsetTL.y = -2, sample offset Y will be [-2, 1], clipbox will be rows [1, 3].
    // Flip row# for sampling offset in this case, so first 0~2 rows in the sampled array can always be used for computing the clipbox.
    // This reduces branch or cmove on sampled colors, but moving this overhead to sample position / weight calculation time which apply to less values.
    let bFlipRow = fSrcUnjitteredPos.y > fSrcOutputPos.y;
    let bFlipCol = fSrcUnjitteredPos.x > fSrcOutputPos.x;

    let fOffsetTL = vec2<f32>(offsetTL);

    for (var row = 0; row < 3; row++) {
        for (var col = 0; col < 3; col++) {
            let iSampleIndex = col + (row << 2u);

            let sampleColRow = vec2<i32>(select(col, 3 - col, bFlipCol), select(row, 3 - row, bFlipRow));
            let iSrcSamplePos = iSrcInputPos + offsetTL + sampleColRow;

            let sampleCoord = ClampLoad(iSrcSamplePos, vec2<i32>(0, 0), RenderSize());

            fSamples[iSampleIndex] = LoadPreparedInputColor(sampleCoord);
        }
    }

    var fColorAndWeight = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    let fBaseSampleOffset = fSrcUnjitteredPos - fSrcOutputPos;

    // Identify how much of each upsampled color to be used for this frame
    let fKernelReactiveFactor = max(fReactiveFactor, f32(params.bIsNewSample));
    let fKernelBiasMax = ComputeMaxKernelWeight() * (1.0 - fKernelReactiveFactor);

    let fKernelBiasMin = max(1.0, ((1.0 + fKernelBiasMax) * 0.3));
    let fKernelBiasFactor = max(0.0, max(0.25 * params.fDepthClipFactor, fKernelReactiveFactor));
    let fKernelBias = mix(fKernelBiasMax, fKernelBiasMin, fKernelBiasFactor);

    let fRectificationCurveBias = mix(-2.0, -3.0, ffxSaturate(params.fHrVelocity / 50.0));

    for (var row = 0; row < 3; row++) {
        for (var col = 0; col < 3; col++) {
            let iSampleIndex = col + (row << 2u);

            let sampleColRow = vec2<i32>(select(col, 3 - col, bFlipCol), select(row, 3 - row, bFlipRow));
            let fOffset = fOffsetTL + vec2<f32>(sampleColRow);
            let fSrcSampleOffset = fBaseSampleOffset + fOffset;

            let iSrcSamplePos = iSrcInputPos + offsetTL + sampleColRow;

            let fOnScreenFactor = f32(IsOnScreen(iSrcSamplePos, RenderSize()));
            let fSampleWeight = fOnScreenFactor * GetUpsampleLanczosWeight(fSrcSampleOffset, fKernelBias);

            fColorAndWeight += vec4<f32>(fSamples[iSampleIndex] * fSampleWeight, fSampleWeight);

            // Update rectification box
            {
                let fSrcSampleOffsetSq = dot(fSrcSampleOffset, fSrcSampleOffset);
                let fBoxSampleWeight = exp(fRectificationCurveBias * fSrcSampleOffsetSq);

                let bInitialSample = (row == 0) && (col == 0);
                RectificationBoxAddSample(bInitialSample, clippingBox, fSamples[iSampleIndex], fBoxSampleWeight);
            }
        }
    }

    RectificationBoxComputeVarianceBoxData(clippingBox);

    fColorAndWeight.w *= f32(fColorAndWeight.w > FSR2_EPSILON);

    if (fColorAndWeight.w > FSR2_EPSILON) {
        // Normalize for deringing (we need to compare colors)
        fColorAndWeight = vec4<f32>(fColorAndWeight.xyz / fColorAndWeight.w, fColorAndWeight.w);
        fColorAndWeight.w *= fUpsampleLanczosWeightScale;

        fColorAndWeight = vec4<f32>(Deringing(*clippingBox, fColorAndWeight.xyz), fColorAndWeight.w);
    }

    return fColorAndWeight;
}

// --- ffx_fsr2_accumulate.h ---------------------------------------------------------------

fn GetPxHrVelocity(fMotionVector: vec2<f32>) -> f32 {
    return length(fMotionVector * vec2<f32>(DisplaySize()));
}

fn Accumulate(params: AccumulationPassCommonParams, fHistoryColor: ptr<function, vec3<f32>>, fAccumulation_in: vec3<f32>, fUpsampledColorAndWeight_in: vec4<f32>) {
    // Aviod invalid values when accumulation and upsampled weight is 0
    let fAccumulation = max(vec3<f32>(FSR2_EPSILON), fAccumulation_in + vec3<f32>(fUpsampledColorAndWeight_in.w));
    var fUpsampledColor = fUpsampledColorAndWeight_in.xyz;

    if (HDR_COLOR_INPUT) {
        // YCoCg -> RGB -> Tonemap -> YCoCg (Use RGB tonemapper to avoid color desaturation)
        fUpsampledColor = RGBToYCoCg(Tonemap(YCoCgToRGB(fUpsampledColor)));
        *fHistoryColor = RGBToYCoCg(Tonemap(YCoCgToRGB(*fHistoryColor)));
    }

    let fAlpha = vec3<f32>(fUpsampledColorAndWeight_in.w) / fAccumulation;
    *fHistoryColor = mix(*fHistoryColor, fUpsampledColor, fAlpha);

    *fHistoryColor = YCoCgToRGB(*fHistoryColor);

    if (HDR_COLOR_INPUT) {
        *fHistoryColor = InverseTonemap(*fHistoryColor);
    }
}

fn RectifyHistory(
    params: AccumulationPassCommonParams,
    clippingBox: RectificationBox,
    fHistoryColor: ptr<function, vec3<f32>>,
    fAccumulation: ptr<function, vec3<f32>>,
    fLockContributionThisFrame: f32,
    fTemporalReactiveFactor: f32,
    fLumaInstabilityFactor: f32) {
    let fScaleFactorInfluence = min(20.0, pow(1.0 / length(DownscaleFactor().x * DownscaleFactor().y), 3.0));

    let fVecolityFactor = ffxSaturate(params.fHrVelocity / 20.0);
    let fBoxScaleT = max(params.fDepthClipFactor, max(params.fAccumulationMask, fVecolityFactor));
    let fBoxScale = mix(fScaleFactorInfluence, 1.0, fBoxScaleT);

    let fScaledBoxVec = clippingBox.boxVec * fBoxScale;
    var boxMin = clippingBox.boxCenter - fScaledBoxVec;
    var boxMax = clippingBox.boxCenter + fScaledBoxVec;

    boxMin = max(clippingBox.aabbMin, boxMin);
    boxMax = min(clippingBox.aabbMax, boxMax);

    if (any(boxMin > *fHistoryColor) || any(*fHistoryColor > boxMax)) {
        let fClampedHistoryColor = clamp(*fHistoryColor, boxMin, boxMax);

        var fHistoryContribution = vec3<f32>(max(fLumaInstabilityFactor, fLockContributionThisFrame));

        let fReactiveFactor = params.fDilatedReactiveFactor;
        let fReactiveContribution = 1.0 - pow(fReactiveFactor, 1.0 / 2.0);
        fHistoryContribution *= fReactiveContribution;

        // Scale history color using rectification info, also using accumulation mask to avoid potential invalid color protection
        *fHistoryColor = mix(fClampedHistoryColor, *fHistoryColor, ffxSaturate3(fHistoryContribution));

        // Scale accumulation using rectification info
        let fAccumulationMin = min(*fAccumulation, vec3<f32>(0.1));
        *fAccumulation = mix(fAccumulationMin, *fAccumulation, ffxSaturate3(fHistoryContribution));
    }
}

fn FinalizeLockStatus(params: AccumulationPassCommonParams, fLockStatus_in: vec2<f32>, fUpsampledWeight: f32) {
    var fLockStatus = fLockStatus_in;
    // we expect similar motion for next frame
    // kill lock if that location is outside screen, avoid locks to be clamped to screen borders
    let fEstimatedUvNextFrame = params.fHrUv - params.fMotionVector;
    if (IsUvInside(fEstimatedUvNextFrame) == false) {
        fLockStatus[LOCK_LIFETIME_REMAINING] = 0.0; // KillLock
    } else {
        // Decrease lock lifetime
        let fLifetimeDecreaseLanczosMax = JitterSequenceLength() * fAverageLanczosWeightPerFrame;
        let fLifetimeDecrease = fUpsampledWeight / fLifetimeDecreaseLanczosMax;
        fLockStatus[LOCK_LIFETIME_REMAINING] = max(0.0, fLockStatus[LOCK_LIFETIME_REMAINING] - fLifetimeDecrease);
    }

    StoreLockStatus(params.iPxHrPos, fLockStatus);
}

fn ComputeBaseAccumulationWeight(params: AccumulationPassCommonParams, fThisFrameReactiveFactor: f32, bInMotionLastFrame: bool, fUpsampledWeight: f32, lockState: LockState) -> vec3<f32> {
    // Always assume max accumulation was reached
    var fBaseAccumulation = fMaxAccumulationLanczosWeight * f32(params.bIsExistingSample) * (1.0 - fThisFrameReactiveFactor) * (1.0 - params.fDepthClipFactor);

    fBaseAccumulation = min(fBaseAccumulation, mix(fBaseAccumulation, fUpsampledWeight * 10.0, max(f32(bInMotionLastFrame), ffxSaturate(params.fHrVelocity * 10.0))));

    fBaseAccumulation = min(fBaseAccumulation, mix(fBaseAccumulation, fUpsampledWeight, ffxSaturate(params.fHrVelocity / 20.0)));

    return vec3<f32>(fBaseAccumulation);
}

fn ComputeLumaInstabilityFactor(params: AccumulationPassCommonParams, clippingBox: RectificationBox, fThisFrameReactiveFactor: f32, fLuminanceDiff: f32) -> f32 {
    let fUnormThreshold = 1.0 / 255.0;
    let N_MINUS_1 = 0;
    let N_MINUS_2 = 1;
    let N_MINUS_3 = 2;
    let N_MINUS_4 = 3;

    var fCurrentFrameLuma = clippingBox.boxCenter.x;

    if (HDR_COLOR_INPUT) {
        fCurrentFrameLuma = fCurrentFrameLuma / (1.0 + max(0.0, fCurrentFrameLuma));
    }

    fCurrentFrameLuma = round(fCurrentFrameLuma * 255.0) / 255.0;

    let bSampleLumaHistory = (max(max(params.fDepthClipFactor, params.fAccumulationMask), fLuminanceDiff) < 0.1) && (params.bIsNewSample == false);
    var fCurrentFrameLumaHistory = select(vec4<f32>(0.0), SampleLumaHistory(params.fReprojectedHrUv), bSampleLumaHistory);

    var fLumaInstability = 0.0;
    let fDiffs0 = (fCurrentFrameLuma - fCurrentFrameLumaHistory[N_MINUS_1]);

    var fMin = abs(fDiffs0);

    if (fMin >= fUnormThreshold) {
        for (var i = N_MINUS_2; i <= N_MINUS_4; i++) {
            let fDiffs1 = (fCurrentFrameLuma - fCurrentFrameLumaHistory[i]);

            if (sign(fDiffs0) == sign(fDiffs1)) {
                // Scale difference to protect historically similar values
                let fMinBias = 1.0;
                fMin = min(fMin, abs(fDiffs1) * fMinBias);
            }
        }

        let fBoxSize = clippingBox.boxVec.x;
        let fBoxSizeFactor = pow(ffxSaturate(fBoxSize / 0.1), 6.0);

        fLumaInstability = f32(fMin != abs(fDiffs0)) * fBoxSizeFactor;
        fLumaInstability = f32(fLumaInstability > fUnormThreshold);

        fLumaInstability *= 1.0 - max(params.fAccumulationMask, pow(fThisFrameReactiveFactor, 1.0 / 6.0));
    }

    // shift history
    fCurrentFrameLumaHistory[N_MINUS_4] = fCurrentFrameLumaHistory[N_MINUS_3];
    fCurrentFrameLumaHistory[N_MINUS_3] = fCurrentFrameLumaHistory[N_MINUS_2];
    fCurrentFrameLumaHistory[N_MINUS_2] = fCurrentFrameLumaHistory[N_MINUS_1];
    fCurrentFrameLumaHistory[N_MINUS_1] = fCurrentFrameLuma;

    StoreLumaHistory(params.iPxHrPos, fCurrentFrameLumaHistory);

    return fLumaInstability * f32(fCurrentFrameLumaHistory[N_MINUS_4] != 0.0);
}

fn ComputeTemporalReactiveFactor(params: AccumulationPassCommonParams, fTemporalReactiveFactor: f32) -> f32 {
    var fNewFactor = min(0.99, fTemporalReactiveFactor);

    fNewFactor = max(fNewFactor, mix(fNewFactor, 0.4, ffxSaturate(params.fHrVelocity)));

    fNewFactor = max(fNewFactor * fNewFactor, max(params.fDepthClipFactor * 0.1, params.fDilatedReactiveFactor));

    // Force reactive factor for new samples
    fNewFactor = select(fNewFactor, 1.0, params.bIsNewSample);

    if (ffxSaturate(params.fHrVelocity * 10.0) >= 1.0) {
        fNewFactor = max(FSR2_EPSILON, fNewFactor) * -1.0;
    }

    return fNewFactor;
}

fn InitParams(iPxHrPos: vec2<i32>) -> AccumulationPassCommonParams {
    var params: AccumulationPassCommonParams;

    params.iPxHrPos = iPxHrPos;
    let fHrUv = (vec2<f32>(iPxHrPos) + 0.5) / vec2<f32>(DisplaySize());
    params.fHrUv = fHrUv;

    let fLrUvJittered = fHrUv + Jitter() / vec2<f32>(RenderSize());
    params.fLrUv_HwSampler = ClampUv(fLrUvJittered, RenderSize(), MaxRenderSize());

    params.fMotionVector = GetMotionVector(iPxHrPos, fHrUv);
    params.fHrVelocity = GetPxHrVelocity(params.fMotionVector);

    // ComputeReprojectedUVs
    params.fReprojectedHrUv = params.fHrUv + params.fMotionVector;
    params.bIsExistingSample = IsUvInside(params.fReprojectedHrUv);

    params.fDepthClipFactor = ffxSaturate(SampleDepthClip(params.fLrUv_HwSampler));

    let fDilatedReactiveMasks = SampleDilatedReactiveMasks(params.fLrUv_HwSampler);
    params.fDilatedReactiveFactor = fDilatedReactiveMasks.x;
    params.fAccumulationMask = fDilatedReactiveMasks.y;
    params.bIsResetFrame = (0 == FrameIndex());

    params.bIsNewSample = (params.bIsExistingSample == false || params.bIsResetFrame);

    return params;
}

// Accumulate(iPxHrPos): the colour to output (the history written)
fn AccumulatePixel(iPxHrPos: vec2<i32>) -> vec3<f32> {
    let params = InitParams(iPxHrPos);

    var fHistoryColor = vec3<f32>(0.0, 0.0, 0.0);
    var fLockStatus = vec2<f32>(0.0, 0.0); // InitializeNewLockSample

    var fTemporalReactiveFactor = 0.0;
    var bInMotionLastFrame = false;
    var lockState = LockState(false, false);
    if (params.bIsExistingSample && !params.bIsResetFrame) {
        let h = ReprojectHistoryColor(params);
        fHistoryColor = h.fHistoryColor;
        fTemporalReactiveFactor = h.fTemporalReactiveFactor;
        bInMotionLastFrame = h.bInMotionLastFrame;
        lockState = ReprojectHistoryLockStatus(params, &fLockStatus);
    }

    var fThisFrameReactiveFactor = max(params.fDilatedReactiveFactor, fTemporalReactiveFactor);

    let lu = UpdateLockStatus(params, &fThisFrameReactiveFactor, lockState, &fLockStatus);
    let fLuminanceDiff = lu.fLuminanceDiff;
    let fLockContributionThisFrame = lu.fLockContributionThisFrame;

    // Load upsampled input color
    var clippingBox: RectificationBox;
    let fUpsampledColorAndWeight = ComputeUpsampledColorAndWeight(params, &clippingBox, fThisFrameReactiveFactor);

    let fLumaInstabilityFactor = ComputeLumaInstabilityFactor(params, clippingBox, fThisFrameReactiveFactor, fLuminanceDiff);

    var fAccumulation = ComputeBaseAccumulationWeight(params, fThisFrameReactiveFactor, bInMotionLastFrame, fUpsampledColorAndWeight.w, lockState);

    if (params.bIsNewSample) {
        fHistoryColor = YCoCgToRGB(fUpsampledColorAndWeight.xyz);
    } else {
        RectifyHistory(params, clippingBox, &fHistoryColor, &fAccumulation, fLockContributionThisFrame, fThisFrameReactiveFactor, fLumaInstabilityFactor);

        Accumulate(params, &fHistoryColor, fAccumulation, fUpsampledColorAndWeight);
    }

    fHistoryColor = UnprepareRgb(fHistoryColor, Exposure());

    FinalizeLockStatus(params, fLockStatus, fUpsampledColorAndWeight.w);

    // Get new temporal reactive factor
    fTemporalReactiveFactor = ComputeTemporalReactiveFactor(params, fThisFrameReactiveFactor);

    StoreInternalColorAndWeight(iPxHrPos, vec4<f32>(fHistoryColor, fTemporalReactiveFactor));

    // (StoreNewLocks(iPxHrPos, 0): see the notes at the top)
    return fHistoryColor;
}

// The reference walks the rows of workgroups bottom up.
fn AccumulateThread(wg: vec2<u32>, lid: vec2<u32>) -> vec2<i32> {
    var uGroupId = wg;
    let GroupRows = (u32(DisplaySize().y) + 8u - 1u) / 8u;
    uGroupId.y = GroupRows - uGroupId.y - 1u;
    return vec2<i32>(uGroupId * vec2<u32>(8u, 8u) + lid);
}

@compute @workgroup_size(8, 8, 1)
fn cs_accumulate(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let iPxHrPos = AccumulateThread(wg.xy, lid.xy);
    let fHistoryColor = AccumulatePixel(iPxHrPos);
    // FFX_FSR2_OPTION_APPLY_SHARPENING 0: WriteUpscaledOutput
    if (InTexture(iPxHrPos, textureDimensions(rw_upscaled_output))) {
        textureStore(rw_upscaled_output, iPxHrPos, vec4<f32>(fHistoryColor, 1.0));
    }
}

@compute @workgroup_size(8, 8, 1)
fn cs_accumulate_sharpen(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    _ = AccumulatePixel(AccumulateThread(wg.xy, lid.xy));
}

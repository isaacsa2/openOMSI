// FSR 2.2.1: the lock pass (ffx_fsr2_lock.h, ffx_fsr2_lock_pass.glsl).
//
// Port notes: the new locks (R8_UNORM at the display size) are an R32F texture - only 0 and 1
// are ever written, so the values are the same. The reference clears each texel in the
// accumulate pass after reading it (a read-write UAV); here the host clears the whole texture
// before this pass instead, so the accumulate pass only samples it: read-write storage textures
// are not available everywhere.

@group(0) @binding(1) var r_lock_input_luma: texture_2d<f32>;
@group(0) @binding(2) var rw_new_locks: texture_storage_2d<r32float, write>;
@group(0) @binding(3) var<storage, read_write> rw_reconstructed_previous_nearest_depth: array<u32>;

fn LoadLockInputLuma(iPxPos: vec2<i32>) -> f32 {
    if (!InTexture(iPxPos, textureDimensions(r_lock_input_luma))) {
        return 0.0;
    }
    return textureLoad(r_lock_input_luma, iPxPos, 0).r;
}

fn StoreNewLocks(iPxPos: vec2<i32>, newLock: f32) {
    if (!InTexture(iPxPos, textureDimensions(rw_new_locks))) {
        return;
    }
    textureStore(rw_new_locks, iPxPos, vec4<f32>(newLock, 0.0, 0.0, 0.0));
}

fn SetReconstructedDepth(iPxSample: vec2<i32>, uValue: u32) {
    rw_reconstructed_previous_nearest_depth[u32(iPxSample.y) * u32(MaxRenderSize().x) + u32(iPxSample.x)] = uValue;
}

fn ClearResourcesForNextFrame(iPxHrPos: vec2<i32>) {
    if (all(iPxHrPos < RenderSize())) {
        // FFX_FSR2_OPTION_INVERTED_DEPTH
        let farZ = 0x0u;
        SetReconstructedDepth(iPxHrPos, farZ);
    }
}

fn SETBIT(x: u32) -> u32 { return 1u << x; }

fn ComputeThinFeatureConfidence(pos: vec2<i32>) -> bool {
    let RADIUS = 1;

    let fNucleus = LoadLockInputLuma(pos);

    let similar_threshold = 1.05;
    var dissimilarLumaMin = FSR2_FLT_MAX;
    var dissimilarLumaMax = 0.0;

    /*
     0 1 2
     3 4 5
     6 7 8
    */

    var mask = SETBIT(4u); // flag fNucleus as similar

    let uRejectionMasks = array<u32, 4>(
        SETBIT(0u) | SETBIT(1u) | SETBIT(3u) | SETBIT(4u), // Upper left
        SETBIT(1u) | SETBIT(2u) | SETBIT(4u) | SETBIT(5u), // Upper right
        SETBIT(3u) | SETBIT(4u) | SETBIT(6u) | SETBIT(7u), // Lower left
        SETBIT(4u) | SETBIT(5u) | SETBIT(7u) | SETBIT(8u), // Lower right
    );

    var idx = 0u;
    for (var y = -RADIUS; y <= RADIUS; y++) {
        for (var x = -RADIUS; x <= RADIUS; x++) {
            // (the reference's `continue` still runs its `idx++`: the counter is in the loop's
            // increment expression there, so the centre keeps index 4)
            if (!(x == 0 && y == 0)) {
                let samplePos = ClampLoad(pos, vec2<i32>(x, y), RenderSize());

                let sampleLuma = LoadLockInputLuma(samplePos);
                let difference = max(sampleLuma, fNucleus) / min(sampleLuma, fNucleus);

                if (difference > 0.0 && (difference < similar_threshold)) {
                    mask |= SETBIT(idx);
                } else {
                    dissimilarLumaMin = min(dissimilarLumaMin, sampleLuma);
                    dissimilarLumaMax = max(dissimilarLumaMax, sampleLuma);
                }
            }
            idx++;
        }
    }

    let isRidge = fNucleus > dissimilarLumaMax || fNucleus < dissimilarLumaMin;

    if (!isRidge) {
        return false;
    }

    for (var i = 0; i < 4; i++) {
        if ((mask & uRejectionMasks[i]) == uRejectionMasks[i]) {
            return false;
        }
    }

    return true;
}

fn ComputeLock(iPxLrPos: vec2<i32>) {
    if (ComputeThinFeatureConfidence(iPxLrPos)) {
        StoreNewLocks(ComputeHrPosFromLrPos(iPxLrPos), 1.0);
    }

    ClearResourcesForNextFrame(iPxLrPos);
}

@compute @workgroup_size(8, 8, 1)
fn cs_lock(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let uDispatchThreadId = wg.xy * vec2<u32>(8u, 8u) + lid.xy;
    ComputeLock(vec2<i32>(uDispatchThreadId));
}

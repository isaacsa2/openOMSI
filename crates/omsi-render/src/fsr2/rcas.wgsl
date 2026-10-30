// FSR 2.2.1: the RCAS pass (ffx_fsr2_rcas.h, ffx_fsr2_rcas_pass.glsl, and FsrRcasF of
// ffx_fsr1.h with FSR_RCAS_DENOISE, the 32-bit path).

struct RcasConstants {
    rcasConfig: vec4<u32>,
};

@group(0) @binding(1) var<storage, read> r_input_exposure: array<f32, 2>;
@group(0) @binding(2) var r_rcas_input: texture_2d<f32>;
@group(0) @binding(3) var rw_upscaled_output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> cbRCAS: RcasConstants;

fn Exposure() -> f32 {
    var exposure = r_input_exposure[0];
    if (exposure == 0.0) {
        exposure = 1.0;
    }
    return exposure;
}

fn RCASConfig() -> vec4<u32> { return cbRCAS.rcasConfig; }

fn LoadRCAS_Input(iPxPos: vec2<i32>) -> vec4<f32> {
    if (!InTexture(iPxPos, textureDimensions(r_rcas_input))) {
        return vec4<f32>(0.0);
    }
    return textureLoad(r_rcas_input, iPxPos, 0);
}

fn FsrRcasLoadF(p: vec2<i32>) -> vec4<f32> {
    var fColor = LoadRCAS_Input(p);
    fColor = vec4<f32>(PrepareRgb(fColor.rgb, Exposure(), PreExposure()), fColor.a);
    return fColor;
}

// This is set at the limit of providing unnatural results for sharpening.
const FSR_RCAS_LIMIT: f32 = 0.25 - (1.0 / 16.0);

// FsrRcasF (FsrRcasInputF is the identity here)
fn FsrRcasF(ip: vec2<u32>, con: vec4<u32>) -> vec3<f32> {
    // Algorithm uses minimal 3x3 pixel neighborhood.
    //    b
    //  d e f
    //    h
    let sp = vec2<i32>(ip);
    let b = FsrRcasLoadF(sp + vec2<i32>(0, -1)).rgb;
    let d = FsrRcasLoadF(sp + vec2<i32>(-1, 0)).rgb;
    let e = FsrRcasLoadF(sp).rgb;
    let f = FsrRcasLoadF(sp + vec2<i32>(1, 0)).rgb;
    let h = FsrRcasLoadF(sp + vec2<i32>(0, 1)).rgb;
    // Rename (32-bit) or regroup (16-bit).
    let bR = b.r;
    let bG = b.g;
    let bB = b.b;
    let dR = d.r;
    let dG = d.g;
    let dB = d.b;
    let eR = e.r;
    let eG = e.g;
    let eB = e.b;
    let fR = f.r;
    let fG = f.g;
    let fB = f.b;
    let hR = h.r;
    let hG = h.g;
    let hB = h.b;
    // Luma times 2.
    let bL = bB * 0.5 + (bR * 0.5 + bG);
    let dL = dB * 0.5 + (dR * 0.5 + dG);
    let eL = eB * 0.5 + (eR * 0.5 + eG);
    let fL = fB * 0.5 + (fR * 0.5 + fG);
    let hL = hB * 0.5 + (hR * 0.5 + hG);
    // Noise detection.
    var nz = 0.25 * bL + 0.25 * dL + 0.25 * fL + 0.25 * hL - eL;
    nz = ffxSaturate(abs(nz) * ffxApproximateReciprocalMedium(ffxMax3(ffxMax3(bL, dL, eL), fL, hL) - ffxMin3(ffxMin3(bL, dL, eL), fL, hL)));
    nz = -0.5 * nz + 1.0;
    // Min and max of ring.
    let mn4R = min(ffxMin3(bR, dR, fR), hR);
    let mn4G = min(ffxMin3(bG, dG, fG), hG);
    let mn4B = min(ffxMin3(bB, dB, fB), hB);
    let mx4R = max(ffxMax3(bR, dR, fR), hR);
    let mx4G = max(ffxMax3(bG, dG, fG), hG);
    let mx4B = max(ffxMax3(bB, dB, fB), hB);
    // Immediate constants for peak range.
    let peakC = vec2<f32>(1.0, -1.0 * 4.0);
    // Limiters, these need to be high precision RCPs.
    let hitMinR = mn4R * (1.0 / (4.0 * mx4R));
    let hitMinG = mn4G * (1.0 / (4.0 * mx4G));
    let hitMinB = mn4B * (1.0 / (4.0 * mx4B));
    let hitMaxR = (peakC.x - mx4R) * (1.0 / (4.0 * mn4R + peakC.y));
    let hitMaxG = (peakC.x - mx4G) * (1.0 / (4.0 * mn4G + peakC.y));
    let hitMaxB = (peakC.x - mx4B) * (1.0 / (4.0 * mn4B + peakC.y));
    let lobeR = max(-hitMinR, hitMaxR);
    let lobeG = max(-hitMinG, hitMaxG);
    let lobeB = max(-hitMinB, hitMaxB);
    var lobe = max(-FSR_RCAS_LIMIT, min(ffxMax3(lobeR, lobeG, lobeB), 0.0)) * bitcast<f32>(con.x);
    // Apply noise removal (FSR_RCAS_DENOISE).
    lobe *= nz;
    // Resolve, which needs the medium precision rcp approximation to avoid visible tonality changes.
    let rcpL = ffxApproximateReciprocalMedium(4.0 * lobe + 1.0);
    let pixR = (lobe * bR + lobe * dR + lobe * hR + lobe * fR + eR) * rcpL;
    let pixG = (lobe * bG + lobe * dG + lobe * hG + lobe * fG + eG) * rcpL;
    let pixB = (lobe * bB + lobe * dB + lobe * hB + lobe * fB + eB) * rcpL;
    return vec3<f32>(pixR, pixG, pixB);
}

fn CurrFilter(pos: vec2<u32>) {
    var c = FsrRcasF(pos, RCASConfig());

    c = UnprepareRgb(c, Exposure());

    if (InTexture(vec2<i32>(pos), textureDimensions(rw_upscaled_output))) {
        textureStore(rw_upscaled_output, vec2<i32>(pos), vec4<f32>(c, 1.0));
    }
}

@compute @workgroup_size(64, 1, 1)
fn cs_rcas(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wg: vec3<u32>) {
    // Do remapping of local xy in workgroup for a more PS-like swizzle pattern.
    var gxy = ffxRemapForQuad(lid.x) + vec2<u32>(wg.x << 4u, wg.y << 4u);
    CurrFilter(gxy);
    gxy.x += 8u;
    CurrFilter(gxy);
    gxy.y += 8u;
    CurrFilter(gxy);
    gxy.x -= 8u;
    CurrFilter(gxy);
}

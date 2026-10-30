# AMD FSR 2.2.1 in WGSL

Graphics → Upscaling → **AMD FSR 2.0 (WGSL, experimental)** resolves the 3D picture with a
port of AMD FidelityFX Super Resolution 2.2.1. The picture is drawn smaller, with a sub-pixel
jitter, and FSR 2 rebuilds it at the window's size from the frames before it, guided by the
depth buffer and the motion vectors.

The port lives in `crates/omsi-render/src/fsr2/`. It is the reference's shaders rewritten in
WGSL and its host code rewritten in Rust. It runs on the renderer's own wgpu device, queue
and command encoder (no second device, no native SDK, no `unsafe`), so it goes wherever wgpu
goes on a device with the limits listed below. FSR 1 (`upscaler=fsr1`) is unchanged.

## Reference

- Repository: GPUOpen-Effects/FidelityFX-FSR2
- Commit: `1680d1edd5c034f88ebbbb793d8b88f8842cf804` (tag v2.2.1)
- Shaders: `src/ffx-fsr2-api/shaders`
- Host code: `src/ffx-fsr2-api/ffx_fsr2.cpp`

The permutation ported is:

- the 32-bit path (`FFX_HALF 0`);
- `HDR_COLOR_INPUT 0`, a pipeline-overridable constant;
- `LOW_RESOLUTION_MOTION_VECTORS 1`;
- `JITTERED_MOTION_VECTORS 0`;
- `INVERTED_DEPTH 1` (the renderer's reversed, finite depth);
- `REPROJECT_USE_LANCZOS_TYPE 0`;
- `UPSAMPLE_USE_LANCZOS_TYPE 2`.

The licence header of the reference (MIT) is kept in `common.wgsl`.

## Correspondence

### Shaders

| Reference | Port | Entry point |
| --- | --- | --- |
| `ffx_fsr2_common.h`, `ffx_fsr2_sample.h` (the helpers used), the `cbFSR2` block of `ffx_fsr2_callbacks_glsl.h` | `common.wgsl` | — |
| `ffx_fsr2_compute_luminance_pyramid.h` + `ffx_spd.h` (LDS path) | `luminance_pyramid.wgsl`, `luminance_pyramid_head.wgsl` | `cs_luminance_pyramid` |
| SPD's last workgroup after `SpdExitWorkgroup` | `luminance_pyramid_tail.wgsl` | `cs_luminance_tail` |
| `ffx_fsr2_reconstruct_dilated_velocity_and_previous_depth.h` | `reconstruct.wgsl` | `cs_reconstruct_previous_depth` |
| `ffx_fsr2_depth_clip.h` | `depth_clip.wgsl` | `cs_depth_clip` |
| `ffx_fsr2_lock.h` | `lock.wgsl` | `cs_lock` |
| `ffx_fsr2_accumulate.h`, `_reproject.h`, `_upsample.h`, `_postprocess_lock_status.h` | `accumulate.wgsl` | `cs_accumulate`, `cs_accumulate_sharpen` |
| `ffx_fsr2_rcas.h` + `FsrRcasF` of `ffx_fsr1.h` (denoise) | `rcas.wgsl` | `cs_rcas` |

The autogen reactive and TCR passes are optional in the reference and are not ported. The
reactive and transparency masks are the reference's 1 x 1 zero defaults, as when a game
gives none.

### Host code

| `ffx_fsr2.cpp` | Port |
| --- | --- |
| `ffxFsr2GetUpscaleRatioFromQualityMode`, `ffxFsr2GetRenderResolutionFromQualityMode` | `params::Quality` |
| `ffxFsr2GetJitterPhaseCount`, `ffxFsr2GetJitterOffset`, `halton` | `params::jitter_phase_count`, `params::jitter_offset` |
| `setupDeviceDepthToViewSpaceDepthParams` | `params::device_to_view_depth` |
| `fsr2Dispatch` (the constants, the history) | `params::dispatch_constants`, `params::History` |
| `ffxSpdSetup` | `params::spd_setup` |
| `FsrRcasCon` | `params::rcas_config` |
| the internal resources (`createResourceFromDescription`) | `resources.rs` |
| the scheduling, the resource ping-pong, the clears on reset | `Fsr2::dispatch` (`mod.rs`) |

The quality modes keep AMD's ratios:

| Mode | Ratio | Render scale |
| --- | --- | --- |
| Quality | 1.5 | 67 % |
| Balanced | 1.7 | 59 % |
| Performance | 2.0 | 50 % |
| Ultra Performance | 3.0 | 33 % |

The jitter is AMD's Halton sequence over `8 × ratio²` phases. It moves only the view
projection the scene is drawn with. The motion vectors are taken without it, which is why
`JITTERED_MOTION_VECTORS` is 0. The HUD is drawn after FSR 2, at the window's size, never
jittered. Texture LOD bias with FSR 2 is `log2(render / display) − 1`, clamped to −3..0.

## Divergences

These are differences in the resources a pass uses. None changes the arithmetic, and each
pass's header says the same.

- **Formats.**
  - R16F, RG16F, R8 and RG8 resources are R32F, RGBA16F or RGBA8 textures.
  - Values the reference stores at half precision are rounded to half explicitly (`asHalf`,
    round to nearest even). This is done in the shader because `pack2x16float` needs an
    optional capability.
  - The reason: WebGPU guarantees storage writes only for a few formats, and at most four
    storage textures per stage.
- **Reconstructed previous depth.** The reference uses an R32_UINT image with image atomics.
  The port uses a storage buffer of atomic `u32`, because image atomics are optional in
  WebGPU. The values and the max are the same.
- **Auto exposure.** A storage buffer of two floats instead of an RG32F image.
- **SPD.**
  - It runs as two dispatches: the head up to `SpdExitWorkgroup`, then the tail as one
    workgroup. The reference's last-workgroup continuation needs a global atomic counter and
    a coherent image read, which WGSL cannot make safe on every backend.
  - The LDS keeps only the red channel, the only one that ever holds a value.
  - Mip 4 (shading change) and mip 5 are separate textures of the reference's sizes.
- **New locks.**
  - The reference clears each texel in the accumulate pass, through a read-write UAV.
  - The port clears the texture on the host before the lock pass, and the accumulate pass
    only samples it, because read-write storage textures are not available everywhere.
  - After a frame, the new locks texture therefore still holds that frame's locks. The output
    is the same.
- **Out of bounds.**
  - Threads past the render size run, as in the reference.
  - Their loads read 0 and their stores are dropped explicitly (`InTexture`). This is what
    D3D12 and Vulkan robustness give the reference.
- **Clears.** Done with render passes (`LoadOp::Clear`), because `CLEAR_TEXTURE` is an optional
  feature.
- **Not ported.** No Lanczos LUT (the reference's own default path computes the kernel), no
  FP16 path, no wave operations.

## Where it runs

`fsr2::supported` checks the device before any pipeline is built:

- 256-thread compute workgroups;
- 1 KiB of workgroup memory;
- 4 storage textures and 2 storage buffers per stage;
- 9 sampled textures and 15 bindings per group;
- shader-writable RGBA16F, R32F and RGBA8.

The format check uses the adapter's own format table when the device has
`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`, and WebGPU's guarantees otherwise. A device
that fails it, a driver that fails to build a pipeline, or MSAA on (FSR 2 needs a
single-sampled depth buffer) leaves FSR 2 out. The game then says so once and the standard
upscaler stays. OpenGL ES 3.0 / WebGL 2 have no compute shaders and never get it.

## Settings

`settings.cfg` (and the launcher, which reads and writes the same keys):

| Key | Values | Default |
| --- | --- | --- |
| `upscaler` | `default`, `taa`, `fsr1`, `fsr2` | `default` |
| `fsr2_quality` | `quality`, `balanced`, `performance`, `ultra_performance` | `quality` |
| `fsr2_sharpness` | 0 .. 1 (0: no RCAS pass) | 0.8 |

Choosing TAA or FSR 2 stores `msaa=1`, and choosing MSAA again turns the upscaler back to
Standard.

FSR 1 keeps its own `fsr_quality` and `fsr_sharpness`.

A render scale below the mode's lowers the render size further. Such a render scale comes
from the optimiser, or from the safer graphics after a lost device.

`upscaler=fsr2` used to select the earlier TAA-based preview. It now selects this port.

## Validation

- **Without a GPU** (`fsr2::tests`, run in CI):
  - Every module parses and validates with no optional capability.
  - Every module translates to SPIR-V, HLSL 5.1 and GLSL ES 3.1.
  - Each pass binds exactly what its shader reads, with matching kinds and formats.
  - The dispatches cover every pixel.
- **On a GPU** (`tests/fsr2.rs`, ignored by default):
  - A still scene resolved over 40 frames is closer to the full-size picture than the
    standard upscaler's. Mean difference on llvmpipe: 1.69 against 2.32.
  - Turning FSR 2 off and resizing work without errors.
- **Against AMD's own shaders** (`fsr2/reference.rs`, ignored by default).
  - The test compiles the reference's GLSL with glslangValidator in the same permutation
    and runs it unchanged as SPIR-V, on resources in the reference's own formats.
  - It drives both the port and the reference with the same inputs: 12 frames of a panning
    picture with a moving square, a reset at frame 7, and RCAS on every other frame.
  - It compares the output, the history and every intermediate.
  - Setup in the GLSL copy:
    - The copy sets three format qualifiers to their resource's format.
    - It defines D3D12's out-of-bounds behaviour.
    - Its bindings are renumbered without gaps. wgpu numbers a layout's descriptors
      contiguously, and a pass-through module is not renumbered.
  - Run it with:

    ```
    OMSI_FSR2_REFERENCE=<FidelityFX-FSR2>/src/ffx-fsr2-api/shaders \
      cargo test -p omsi-render --lib fsr2::reference -- --ignored --nocapture
    ```

  Result on Mesa lavapipe (llvmpipe, LLVM 20.1.2):

  | Render → display | Frames | Output / history difference |
  | --- | --- | --- |
  | 96 × 64 → 144 × 96 | 12 | 0 (bit-identical) |
  | 97 × 61 → 194 × 122 | 12 | 0 |
  | 75 × 41 → 224 × 123 | 12 | 0 |
  | 113 × 67 → 170 × 101 | 12 | 0 |
  | 320 × 180 → 480 × 270 | 12 | 0 |

  - **Intermediates.** All match except the lock input luma, which can differ by one half
    ULP (≤ 4.9e-4). That is a rounding difference of the R16F store, and it does not reach
    the output.
  - **Control.** With `OMSI_FSR2_REFERENCE_ONLY=port`, where the reference does not run, the
    same comparison fails (mean 0.47). So it compares two real computations.
  - **Limit.** A 1 × 1 render size is not comparable: the reference's luminance mips 4 and 5
    do not exist there.

## Performance

Not measured yet. The numbers above come from a CPU rasteriser and say nothing about speed.
Measuring needs real GPUs. The per-pass GPU time is in the performance overlay, under the
`FSR2` timer. On each GPU to measure:

1. Note FPS and the `FSR2` time at 1080p and 1440p for Quality and Performance.
2. Compare with Standard and FSR 1 at the same render scale.

## Known limits

- No reactive or transparency mask yet. Rain on glass, particles and animated textures can
  ghost.
- The motion pass gives no per-instance motion yet to animated traffic, hands, wipers and
  passengers (`docs/temporal-upscaling.md`). They can smear.
- On wet roads with puddles, the main pass draws into a depth buffer of its own, so that
  its depth-writing glass does not cover the road the puddles reflect. The motion vectors
  and FSR 2 read the prepass's depth instead: the opaque surfaces, which is what AMD asks
  for. FSR 2 stays on in the rain. Glass then moves with what lies behind it, as it does
  without a reactive mask anyway.
- A frame with a presurface (an excavation) in it, or without a depth prepass, goes to the
  standard upscaler without the projection jitter, and the history starts afresh.
- HDR input (`HDR_COLOR_INPUT 1`) is wired as a pipeline constant but not used: the picture
  is tone-mapped before FSR 2.
- MSAA must be off, and turning it off takes a restart.
- The Apple settings page offers FSR 2 and its quality/sharpness settings, along with
  TAA. Metal uses the WGSL port, but real Apple GPU performance/compatibility has not
  yet been measured. Selecting a temporal mode also disables MSAA for the next launch.

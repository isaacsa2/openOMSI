# Temporal upscaling (towards FSR 2), step by step

FSR 1 (`docs/fsr1-experiment.md`) is *spatial*: it scales up one picture at a time. FSR 2 and
any temporal anti-aliasing build each picture from several: they need, for every pixel, where it
was in the last picture (motion vectors), a camera that moves by a fraction of a pixel from one
picture to the next (jitter), the depth, and to know what cannot be followed (glass, rain,
particles). None of that existed. It is added in steps, each its own pull request, each off by
default and testable on its own; FSR 1 stays as it is beside it, and the upscaler setting will
offer FSR 2 as another choice, never in its place.

## Step 1: motion vectors (`OMSI_MOTION_VECTORS=1`)

`passes/motion.rs`, `motion.wgsl`. After the main pass, a full-screen pass works out from the
depth where each pixel of the window's 3D picture was a frame ago, as last frame's UV minus this
frame's (`Rg16Float`, FSR 2's convention).

- **The world** is taken as still and the camera as moving: the point is rebuilt from the depth
  and projected with last frame's view-projection (moved by the change of render origin). The
  depth is reversed (near 1, far 0) and the reprojection homogeneous, so the sky, at depth 0,
  moves by the camera's turn alone.
- **The player's vehicle** moves with the camera: a point inside its box (`Lighting::inside`) is
  carried back with the vehicle, so the cab keeps its place on the screen instead of smearing at
  every turn - the reprojection the ray tracer's history used before it was taken out
  (`c81c94de`).
- **Not yet in it:** what moves by itself - the traffic, the people, the bus's own wipers, doors
  and needles, the articulated bus's bellows. They need each instance's transform of the last
  frame (step 4).
- **Where it runs:** where the main pass's depth is the single-sampled one of the prepass
  (`shares_prepass_depth`: no MSAA, no puddles or presurfaces that frame). With the vectors on, the
  prepass is drawn even where nothing else asked for it (vanilla without SSAO): one depth pass
  more. The depth is read as a plain float texture, so OpenGL takes it too.
- **History validity:** discard stale frames after a resize, camera cut,
  teleport, projection change, or a stall longer than 0.5 seconds. Reuse the
  current renderer's `inside_rotation` for pitch and bank, without duplicate state.
- **Nothing reads the vectors yet**: the picture is unchanged. `OMSI_DEBUG_MOTION=1` (or a
  number: the pixels of motion at full colour, 8 by default) draws them over the 3D picture -
  red across, green up and down - to check them in the cab and on a turn.
- **Tests** (on a graphics adapter, `--ignored`): a still camera has no motion; after a step and
  a turn, more than 99 % of a rail's pixels lead back along their vector to within a pixel of
  the rail in the picture before (where staying put would miss for over 40 %), and the sky moved
  by the turn; a dashboard inside the vehicle that moves with the camera has no motion while the
  road outside does. And the shader makes GLSL.

## Next steps

2. **Jitter**: the projection moved by a sub-pixel offset each frame (a Halton sequence), and
   the offset handed on - off unless asked for, as it shakes a picture nothing accumulates.
3. **TAA**: a temporal anti-aliasing pass on top - the history reprojected along the vectors,
   clamped to the neighbourhood, rejected by depth - to prove the vectors right in practice
   before anything bigger rests on them.
4. **Moving objects**: each instance's transform of the last frame, so the traffic, the people
   and the cab's moving parts get vectors of their own.
5. **FSR 2** itself (or the TAA upscaling the picture), as a choice of the upscaler setting next
   to the standard upscaler and FSR 1, with a reactive mask for glass, rain and particles.

## FSR 2 backend handoff (staged)

`crates/omsi-render/src/temporal.rs` defines a vendor-neutral frame contract, including
the reversed-Z depth flag, render and presentation extents, motion scaling, jitter
sequence, and reset signals. The motion-pass now tracks whether it produced input
this frame and whether it has a previous valid view. Its GPU resources can be
borrowed for an actual FSR 2 dispatch once an AMD FidelityFX backend exists.

**There is no AMD FidelityFX SDK/context or FSR 2 output in this branch yet.**
Do not expose an "FSR 2 active" menu item until a backend compiles, dispatches
and passes image/performance tests on real Vulkan/DX12 devices. Do not enable
jitter without a temporal resolver: an image without reconstruction would shake.

- Temporal algorithms are mutually exclusive. FSR 1 must not combine with a
  standalone TAA or a temporal upscaler; an unavailable FSR 2 / DLSS may fall
  back to working TAA only after that independent backend is implemented.
- The existing motion values are *normalized UV differences*, not render
  pixels. FSR 2 must use the per-frame `motion_vector_scale` rather than
  submitting these unscaled; depth uses reversed Z.
- The frame's color, depth and motion must come from the same desktop main view
  at render resolution and the same view projection. Currently animated
  traffic, hands, wipers and passengers still need per-instance motion.
- Reactive and transparency/composition masks are currently unavailable.
  Before enabling production FSR 2, generate them for rain, glass, smoke and
  moving semitransparent surfaces and validate with real modded buses.
- Native Vulkan/DX12 access, resource-state transitions, official SDK linkage,
  temporal output and full in-game performance validation remain required.

## Experimental temporal AA test consumer (not AMD FSR 2)

Set `OMSI_TAA=1` to run an experimental temporal history filter, disabled by
default and excluded when FSR 1 is active or MSAA exceeds 1. It samples the
engine's motion vectors and two ping-pong history images, using a neighborhood
clamp and velocity-weighted feedback. It draws before the spatial upscaler,
keeping the HUD outside the history. No synchronous GPU readback is involved.

The temporal filter currently runs **without camera jitter**: the Halton
generator and clip-space conversion are implemented and tested, but enabling
jitter before there is robust reconstruction for every render path would
introduce visible shaking. This filter should be considered a *validation
consumer / fallback prototype*, not final production TAA or AMD FSR 2.

Discontinuities, invalid motion and resolution changes clear history. Mirrors,
VR, and extra views never update the main history. Expected limitations:
moving AI, animated wipers, partially transparent windows and rain need
per-object motion and reactive/transparency masks, and may still ghost.

Use `OMSI_MOTION_VECTORS=1 OMSI_DEBUG_MOTION=1` without TAA to inspect the
original vectors directly; disable the flags to restore the existing pipeline.

### Engine-owned resource handoff

After the 3D tone mapping and motion pass, `Renderer::fsr2_frame_inputs` now
validates and borrows actual motion/depth/color texture views and the render
and output dimensions. It refuses offscreen mirrors, XR eyes, stale motion,
device-loss or absent scaled scene buffers. This is a private staging API,
not an AMD SDK context, and does not perform a native FSR 2 dispatch.

The experimental TAA conservatively lowers its history contribution when rain
or wet glass is present; it does not invent a per-pixel reactive mask. Switching
back to FSR 1 releases TAA and its motion pass when no independent motion-vector
debugging flag was requested, avoiding an unnecessary per-frame GPU cost.

## Select temporal preview through the menu

The Launcher and in-game Graphics → Upscaling menu now offer four distinct modes:

- **Standard** (unchanged).
- **AMD FSR 1**: the existing spatial EASU/RCAS implementation is retained.
- **TAA (experimental)**: standalone motion-driven history at the current render scale.
- **FSR 2 Preview (TAA-based, not AMD FidelityFX)**: selects the quality
  scale (Ultra/Quality/Balanced/Performance) and processes the reduced-resolution
  picture with the experimental TAA history, then the existing spatial scaler.
  **This is a playable temporal *prototype*, not genuine AMD FSR 2**.

FSR 1, TAA and the FSR 2 preview are mutually exclusive. Selecting a temporal
option also stores MSAA=Off (required for the current motion-vector pass); selecting
MSAA 2x/4x/8x while one is selected switches the upscaler to Standard. Existing
FSR 1 settings, shader compilation, and quality presets remain available.
Temporal settings are saved in `settings.cfg`, and switching the upscaler in-game
attempts to update the active renderer without forcing a restart (switching the
actual multisample render target still requires a restart).

For testing, compare unchanged Standard vs FSR 1 vs Preview at the same quality
and weather. The preview lacks real AMD SDK dispatch, jittered scene rendering,
reactive masks, and animated-instance motion; it can ghost, so it is opt-in.

## Genuine FidelityFX backend comparison (2026-10-29)

FSR 1 EASU/RCAS already runs in **WGSL** (`fsr1.wgsl`). Do not confuse
`FSR 2 Preview`, which currently uses a custom TAA and spatial stretch, with AMD FSR 2.
The official AMD SDK and `sp-fidelity-wgpu` (unofficial 1:1 WGSL port of AMD FSR 2)
are **two distinct backends**; neither is currently executing in the game.

- **Official AMD FSR 2:** native Vulkan/DX12 interop, resource ownership and GPU
  synchronization need to be implemented without CPU readback. Do not link an
  unverified native SDK directly to GL/ANGLE/Android.
- **FSR 2 WGSL (sp-fidelity 0.2):** the upstream port depends on `wgpu 30.0.1`,
  while openOMSI pins a patched fork of `wgpu 29.0.4`. Our fork backports
  several v30 fixes, but these are *not* proof of source/API compatibility.
  The backend also requires SUBGROUP, TEXTURE_ATOMIC, TEXTURE_FORMAT_16BIT_NORM,
  TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, and FLOAT32_FILTERABLE. Query these
  on each adapter before trying to create a FidelityFX device. A blind
  dependency upgrade risks pulling in a second incompatible `wgpu` version.
- **FSR 3 Upscaler (wgpu-ffx 0.1):** separate, early wgpu 29 implementation;
  requires `Rgba16Float` colour and output, `Depth32Float`, `Rg16Float`
  motion and extra intermediate compute resources. Generates no extra frames;
  this is **upscaling only**. Its reactive-mask generation pass is unfinished.
  The `fsr3-wgpu` Cargo feature adds its dependency but deliberately does
  **not** make it available in the default build until the full dispatch path,
  shader validation, actual jitter, resizes, device-loss and low-VRAM tests pass.

**Priority:** keep FSR 1 and Standard unchanged, validate a complete FSR 3
upscaling dispatch on Vulkan/DX12/Metal, then port the FSR 2 WGSL backend or
expose the official FSR 2 native SDK as alternate, mutually exclusive choices.
All require same-frame, correctly oriented motion vectors, true projection
jitter, depth, color and an unjittered HUD. A frame where any input is absent
must fall back rather than claim that FSR 2/3 is running.

## Experimental real FSR3 integration

`--features fsr3-wgpu` compiles the actual wgpu-ffx 0.1.0 implementation of
AMD's **FSR3 temporal upscaler** (not frame generation). The scene's presentable
colour is converted into a linear RGBA16F input, then the backend records its
compute passes with the same-frame reversed-Z Depth32Float and RG16Float
motion, dynamic resolution, jitter and output-size metadata. The resulting
RGBA16F texture is presented at native size before the HUD, without FSR1 or TAA.

The backend is off by default, not built for ordinary users; the menu only
offers the in-game FSR3 option when the backend feature is compiled. The
launcher labels it as requiring an experimental build. If any dependency
or GPU driver rejects the backend, the renderer falls back to its existing
scaler. Real GPU and performance checks are necessary before enabling
`fsr3-wgpu` by default. This does not implement either of the FSR2 variants.

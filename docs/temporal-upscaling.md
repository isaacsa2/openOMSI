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

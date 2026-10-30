# Temporal upscaling (TAA and FSR 2), step by step

FSR 1 (`docs/fsr1-experiment.md`) is *spatial*: it scales up one picture at a time. FSR 2 and
any temporal anti-aliasing build each picture from several: they need, for every pixel, where it
was in the last picture (motion vectors), a camera that moves by a fraction of a pixel from one
picture to the next (jitter), the depth, and to know what cannot be followed (glass, rain,
particles). The renderer now implements motion vectors, experimental TAA, and a WGSL port
of AMD FSR 2.2.1, each opt-in and testable on its own. FSR 1 stays available beside
these modes, not replaced by them.

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
- **Where it runs:** where there is a single-sampled prepass depth of the picture's size
  (`motion_depth_ready`: no MSAA, no presurface that frame; with puddles the prepass's depth
  is read, though the main pass draws into its own). With the vectors on, the
  prepass is drawn even where nothing else asked for it (vanilla without SSAO): one depth pass
  more. The depth is read as a plain float texture, so OpenGL takes it too.
- **History validity:** discard stale frames after a resize, camera cut,
  teleport, projection change, or a stall longer than 0.5 seconds. Reuse the
  current renderer's `inside_rotation` for pitch and bank, without duplicate state.
- **Consumers:** FSR 2 and standalone TAA read these vectors when selected.
  `OMSI_DEBUG_MOTION=1` (or a number: pixels of motion at full colour, 8 by default)
  draws them over the 3D picture - red across, green up and down - for testing.
- **Tests** (on a graphics adapter, `--ignored`): a still camera has no motion; after a step and
  a turn, more than 99 % of a rail's pixels lead back along their vector to within a pixel of
  the rail in the picture before (where staying put would miss for over 40 %), and the sky moved
  by the turn; a dashboard inside the vehicle that moves with the camera has no motion while the
  road outside does. And the shader makes GLSL.

## Progress and remaining work

2. **Jitter - implemented for FSR 2:** AMD's Halton sequence shifts the main picture's
   projection only when FSR 2 is selected; motion vectors remain unjittered.
3. **TAA - implemented as an experimental standalone option:** the history is reprojected
   along motion vectors and clamped, without FSR 2's AMD reconstruction.
4. **Moving objects - pending:** per-instance transforms are still needed for traffic,
   passengers, doors, wipers and other animated geometry.
5. **FSR 2 - implemented experimentally:** AMD FSR 2.2.1 is ported to WGSL and selectable
   beside Standard, TAA and FSR 1. Reactive and transparency masks remain pending.

## FSR 2 backend handoff (staged)

`crates/omsi-render/src/temporal.rs` defines a vendor-neutral frame contract, including
the reversed-Z depth flag, render and presentation extents, motion scaling, jitter
sequence, and reset signals. The motion pass tracks whether it produced valid input this frame and whether it
has a usable previous view. Its GPU resources are borrowed by the existing WGSL
FSR 2 dispatch, which also checks device capability and resets history when needed.

FSR 2 itself is in [fsr2.md](fsr2.md): a WGSL port of AMD FidelityFX FSR 2.2.1 that
reads these resources and jitters the projection it is drawn with. Do not enable jitter
without a temporal resolver: an image without reconstruction would shake.

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
  Before treating FSR 2 as production-ready, generate them for rain, glass,
  smoke and moving semitransparent surfaces and validate with real modded buses.
- In-game performance on real GPUs is still to be measured (fsr2.md).

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
device-loss or absent scaled scene buffers. `Renderer::encode_fsr2` hands them to
the FSR 2 port (fsr2.md).

The experimental TAA conservatively lowers its history contribution when rain
or wet glass is present; it does not invent a per-pixel reactive mask. Switching
back to FSR 1 releases TAA and its motion pass when no independent motion-vector
debugging flag was requested, avoiding an unnecessary per-frame GPU cost.

## Select the upscaler through the menu

The launcher, Apple settings page and in-game Graphics → Upscaling menu offer four modes:

- **Standard (Catmull-Rom)**: unchanged.
- **TAA (experimental)**: standalone motion-driven history at the current render scale.
- **AMD FSR 1.0**: the existing spatial EASU/RCAS implementation, unchanged.
- **AMD FSR 2.0 (WGSL, experimental)**: AMD FSR 2.2.1 ported to WGSL (fsr2.md), with AMD's
  quality modes and its own RCAS sharpness.

They are mutually exclusive. Selecting a temporal option (TAA, FSR 2) also stores
MSAA=Off, which the motion-vector pass requires. Selecting MSAA 2x/4x/8x while one is
selected switches the upscaler to Standard. The settings are saved in `settings.cfg`, and
switching the upscaler in-game updates the running renderer without a restart. Switching the
multisampled render target itself still needs a restart.

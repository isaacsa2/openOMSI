# Investigating the 1166–1169 regression (#1091)

The tags are `v0.1.1166` (`1f648cd`) and `v0.1.1169` (`dd24a62`).
There are three commits between them: a pause notice removal, sound voice ranking,
and the missing `[matl_transmap]` alpha-stage fix. Only the last changes the renderer:
it sets `MaterialUniform.params.z` for a declared but missing map too. It does not
change alpha mode, draw-list ordering, batches, geometry, or pass scheduling.
The shader then supplies alpha 1 rather than using diffuse alpha. This can change
fragment coverage; it must not be reverted just to regain FPS.

The controlled map/vehicle benchmark has not been reproduced in the cloud:
the reported maps and a hardware graphics adapter are unavailable. Shader validation
and software rasterization are correctness checks, not hardware FPS measurements.
The transmap change is a candidate, not a confirmed explanation of the full drop.

The refreshed branches use official main `2ec1b9e`. Earlier builds based on
`1ef9649` do not include the subsequent triple-screen and LAN changes and must not
be mixed with these builds in the same A/B comparison.

## Profiling

Use a real window, the same executable settings, map, bus, spawn, camera, date,
weather, traffic target, seed, resolution and backend for every run. Keep VSync and
the frame limit identical. Let loading finish before measuring; collect at least
30 one-second FPS samples and repeat runs in alternating order. Record hardware,
driver and executable commit along with the logs. Offscreen readback waits and GPU
clock changes make offscreen timing unsuitable as a substitute for window FPS.

Set `RUST_LOG=info`, `OMSI_PROFILE=1`, `OMSI_GPU_TIMERS=1`, and a fixed `OMSI_SEED`.
Use `--exit-after` so the existing shutdown report includes CPU stages, counts and
GPU times. `scripts/profile-th-wald.ps1` is an existing fixed-scene example; its
TH_Wald scene is not the unidentified scene in #1091. For the report's Top Gear and
Lemmental cases, record the exact spawn and camera before comparing the tags.

The pipeline counts describe the submitted main mesh batches, including the cab:

- `batches`: instanced draw calls, including cull/depth-bias variants.
- `draws`: mesh-range instances before instancing, not API draw calls.
- `ktris`: submitted triangle instances in thousands, not visible pixels.
- `opaque`, `alpha-test`, `blend`, `blend-no-write`, `surface-depth`: actual
  selected pipeline, including opaque slots that happen to be in model order.
- Mirror counts are summed across mirror pictures per window frame in the app's
  shutdown report, not divided by the number of mirror pictures.
- Shadow totals include all three cascades. Main asset audits include the cab.
- Triple-screen main counters sum submissions across all three panels. Visible
  instances are per-view submissions, not distinct objects across the panels.
  The periodic asset audit samples one panel, not the whole triple-screen frame.
  XR pipeline counters belong to main views, not mirror pictures; the existing
  CPU/GPU timer labels still group XR eyes with the offscreen path.
- `blend sort` covers key preparation and sorting. It is a subset of `items` CPU
  time; do not add it to that stage total. Ordered opaque slots participate too.

Every ten seconds, `blend audit` lists the twelve source/material pairs with the
most blended range instances, breaking ties by triangles. It prints material and
diffuse texture IDs, declared vs loaded transmap, alpha mode and depth/layer flags.
A declaration or an alpha channel alone does not prove visual transparency:
instance fades, valid mask texels, shader glass response and authored order matter.
Use the existing material debug dump to resolve IDs while inspecting a suspect bus.
Audits describe one sampled frame, not an accumulated per-asset average.

GPU timers already cover main, mirrors, prepass, shadows and post passes when the
adapter supports timestamp queries. Main opaque/alpha/blend draws share a render
pass and ordered phases, so there is no independent GPU timer for each kind.
`OMSI_SKIP_PIPE=0`, `1`, `2`, or `3` with `OMSI_GPU_TIMERS_RAW=1` can estimate its
marginal cost in separate diagnostic runs. Removing a kind changes occlusion and
work behind it: these differences are not additive per-pipeline timings or final
performance results. Shadows/prepass remain enabled during this diagnostic.
There is no direct pixel-overdraw counter; inspect a graphics capture if fragment
cost rises without a corresponding increase in triangles or draw calls.

## Further work

Existing main already caches transformed instance bounds with invalidation, batches
compatible opaque/cutout splines, hashes nearest-object transparency distances,
limits mirror picture frequency, and staggers shadow/probe updates. Preserve those
changes. Do not add mirror resolution cuts, shadow LOD or distant AI throttling
until the corresponding CPU/GPU stage is measured on the affected scene.

`.surf` was added after these tags. `OMSI_NO_SURF=1` remains an A/B diagnostic.
DriveGrid already restricts queries to a spatial cell, but samples relevant faces
in it. A last-triangle wheel cache would have to preserve both above/below hits and
higher overlapping roads, ridges, terrain, multi-point tyre probes and tile reloads.
Still being inside the last triangle does not prove it remains the winning contact.
No such cache is introduced without a demonstrated physics bottleneck.

## Occlusion culling feasibility (proposal only)

Hi-Z could help dense streets where many opaque mesh batches remain behind buildings;
no percentage or FPS gain is established for these maps. It adds a depth reduction
chain, conservative bounds tests and a way to filter/rebuild draw lists. It is a
substantial renderer change, not part of this regression fix.

Integration would follow a suitable opaque depth pass and precede opaque main draw
submission. The renderer uses reversed depth (`GreaterEqual`), so the hierarchy
must retain conservative farthest depth (minimum, including empty pixels) rather
than blindly copying a conventional-depth maximum reduction. MSAA samples and
alpha-tested holes must not create false solid occluders. Current prepasses exclude
some cutouts/blends; authored ground composition cannot be used as fully opaque
until its surface-depth phase has finished. Mirrors need their own view/depth.

DX12 and Vulkan are plausible compute/indirect candidates after adapter capability
checks. OpenGL/GLES support varies: main now explicitly supports devices without
vertex storage buffers. Gate compute/storage/indirect requirements on downlevel
capabilities and keep the existing CPU/frustum path as fallback. A render-pass
pyramid can avoid some compute requirements but does not solve draw compaction.
Same-frame CPU readback would stall; previous-frame results need conservative
motion/streaming invalidation to avoid popping. Windows, Vulkan and older GLES
hardware all require separate validation before this could become a default.

Primary references: [wgpu downlevel capabilities](https://docs.rs/wgpu/29.0.4/wgpu/struct.DownlevelFlags.html),
[wgpu query types](https://docs.rs/wgpu/29.0.4/wgpu/enum.QueryType.html), and
[WebGPU specification](https://gpuweb.github.io/gpuweb/). Occlusion queries are an
alternative diagnostic, but per-object query overhead and delayed readback still
need measurement; they do not provide a free visibility list for this CPU batcher.

# wgpu 30 comparison

This branch uses stable wgpu 30.0.1 with the openOMSI compatibility patches carried
forward from the v29 fork. ANGLE/D3D11 remains available alongside native GL, Vulkan,
DX12 and Metal. The dependency is pinned to an immutable commit in the test fork.

Compare this build against PR #254 on the same machine, map, bus, driver camera,
window dimensions and graphics settings. Keep the game in the foreground. Repeat
each run; the dependency upgrade alone is not evidence of an FPS improvement.

## Capture

Use the same launch arguments and profiling commands as the baseline. For an
additional diagnostic run on native GL or ANGLE, set:

```powershell
$env:OMSI_PROFILE = "1"
$env:OMSI_GPU_TIMERS = "1"
$env:WGPU_GL_PROFILE = "1"
$env:RUST_LOG = "info"
Remove-Item Env:OMSI_PROFILE_GPU -ErrorAction SilentlyContinue
```

Keep `OMSI_PROFILE_JSON` and `--exit-after` set as in the baseline capture. ANGLE
needs enough time for loading before the measured session begins. The GL diagnostic
is disabled by default; disable it again for the main FPS comparison:

```powershell
Remove-Item Env:WGPU_GL_PROFILE -ErrorAction SilentlyContinue
```

## Interpret the measurements

| Measurement | Meaning |
| --- | --- |
| `acquire.surface` | CPU wall time in `get_current_texture`, nested inside `acquire`. |
| `present.encode` | Optional sRGB conversion and its submission, nested inside `present`. |
| `present.surface` | CPU wall time in the queue's surface presentation, nested inside `present`. |
| `render.timer_readback` | Collecting/resolving GPU timestamps, including additional submissions. |
| `render.mirror.timer_readback` | The same operation when called from a mirror render. |
| `render.submit` / `render.mirror.submit` | Time in the public wgpu queue submission, including backend work and upkeep. |
| `GL submit: ... context` | Time acquiring the GL context, plus the small setup before it. |
| `GL submit: ... reset` | State restoration between command buffers. |
| `GL submit: ... commands` | CPU command replay and driver calls, including any waits inside them. |
| `GL submit: ... fence` | Fence maintenance and signaling. |
| `GL submit: ... flush` | The GL flush call. |

The GL log reports averages per submission over roughly one-second intervals and
the largest measured submission. It aggregates GL queues in the process; it is not
a GPU timestamp or a per-frame average. Empty submissions are included.

The overlay's Drawing category groups `render` and `mirrors`. Graphics card groups
`acquire`, `present` and explicit GPU waits. A driver wait inside submission stays
under Drawing. The categories locate elapsed time; they do not prove which device
is saturated. Nested measurements must not be added to their parent totals, and
CPU wall time must not be added to overlapping GPU pass times.

Timestamp readback used to run before the renderer's detailed stage clock started.
It was included in the app's Drawing total but missing from the renderer's detailed
breakdown. The new readback counters expose that gap without changing the work.

Check mirror updates, HUD/navigation, resizing, FXAA/render scaling, weather,
screenshots, bus previews and startup. Windows/ANGLE, Radeon native GL and OpenXR
need platform testing; a successful Linux compile cannot validate their drivers.

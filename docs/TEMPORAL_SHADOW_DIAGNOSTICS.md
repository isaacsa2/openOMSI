# Diagnosing missing shadows with temporal upscaling

Related reports: #3809, #4040, #4221, #4271 and #4345.

The existing `OMSI_DEBUG_SHADOW_FAR=1` flag now also records the main window's shadow state and encoded cascade batch counts on both wgpu and mX. Enable only for short diagnostic captures; per-frame logging has a cost. Normal play is unchanged.

## Controlled A/B

Use the same map, bus, camera, time, weather, preset, render API and shadow settings. Start each run from a fresh process. Disable frame generation and the automatic optimiser for this test. Record the exact build SHA, GPU/driver, backend, game log and a short video.

1. Native rendering, 1x MSAA, 100% render scale.
2. DLAA (native), then DLSS Quality.
3. FSR 3 Native, then FSR 3 Quality.
4. Repeat a failing case with `OMSI_SHADOW_NEAR_EVERY_FRAME=1` and separately with `OMSI_SHADOW_FAR_EVERY_FRAME=1`.
5. If Enhanced+ is affected, repeat with `OMSI_NO_RT_FRAME=1`; also test Enhanced without ray tracing.
6. Repeat with DX12/Vulkan and wgpu/mX where supported. With `OMSI_PROFILE=1`, compare VRAM and texture-budget logs in the affected run.

In PowerShell, before starting the game from that process:

```powershell
$env:OMSI_DEBUG_SHADOW_FAR = "1"
$env:OMSI_PROFILE = "1"
```

## Interpreting the output

- `shadow state`: whether the main view requests and schedules sun/moon shadows, whether temporal upscaling and ray tracing are active, effective shadow size, cascade update decisions, sun direction/intensity and render size.
- `shadow batches`: the number of batches submitted to the shadow-map encoder for each cascade. A zero count for a cached near/far cascade is expected when no redraw is scheduled; a zero count is not proof that an object has stopped casting shadows. In ray-traced mode some opaque casters are intentionally omitted from raster maps.
- If settings remain enabled and the close cascade still has batches while shadows disappear, compare the scene before temporal reconstruction with the final image; inspect depth, motion, and ray-traced lighting next.
- If maps shrink or a rebuild occurs, correlate with OOM and renderer fallback messages; do not infer a renderer failure from FPS alone.

This instrumentation is diagnostic, not a confirmed visual fix. GPU validation on the affected Windows configuration is still required.

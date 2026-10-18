# FSR 1 test build (opt-in)

This branch adds AMD FidelityFX Super Resolution **1.x spatial upscaling** using EASU + RCAS, using shaders derived from AMD's MIT-licensed reference. No native driver DLLs are required.

## Enable and compare

FSR1 is **disabled by default**. Open **Settings > Graphics > Upscaling** in the launcher (or **Esc > Options > Graphics** inside the game) and choose **AMD FSR 1 (experimental)**. Choose an **FSR quality** preset (Ultra Quality 77%, Quality 67%, Balanced 59%, Performance 50%, or Custom) and **FSR sharpening** (0-100%). Settings are persisted in `settings.cfg`, included in graphics profiles and take effect the next time the game starts.

The previous `OMSI_FSR1=1` environment override also still works for developer testing. With the override, the existing **Render scale** setting controls the internal resolution.

- For a quick 1080p test, select **Quality**; the game will use a 0.67 internal scale.
- Ultra Quality: 0.77; Balanced: ~0.59; Performance: 0.50 (the renderer clamps fixed scales to 0.5).
- FSR is only used for scaled **window** rendering. The unchanged Catmull-Rom upscaler is used elsewhere and whenever FSR is disabled or no scaling is required.
- RCAS sharpening is controlled by the FSR sharpening slider (defaults to 30%).
- On Windows PowerShell: `$env:OMSI_FSR1='1'; .\\openomsi.exe` (adjust executable path). Restart after changing the variable.
- To disable for comparison: `Remove-Item Env:OMSI_FSR1`, then relaunch.

## Technical design

- Source picture is tone mapped, then **EASU** writes into a cached full-sized color target.
- **RCAS** reads the cached target and writes the final window image.
- HUD/overlay quads are drawn after RCAS at native resolution.
- Uses WGSL textureLoad (no textureGather requirement), no changes to the scene shaders, wgpu fork, multiplayer or game asset loading.
- The temporary image is cached by window resolution rather than allocated per frame; all FSR shader compilation is skipped unless explicitly enabled at launch.
- Existing AA/render scale settings remain in charge; for best image quality test with MSAA and avoid noisy inputs.

## Remaining validation

This is an experimental implementation and should **not** be merged until:
1. `cargo fmt --check`, `cargo test -p omsi-render --lib` and an application build pass.
2. WGSL pipeline construction is exercised on DX12, Vulkan, ANGLE and OpenGL, including an older GPU.
3. Visual A/B comparisons cover routes, distant text, wires, foliage, bus destination displays and nighttime lighting.
4. GPU timings and memory usage are compared at the **same internal resolution** against the old scaler, with MSAA and backend held fixed.
5. Verify resize, minimized/restored windows, and a runtime fallback when shader compilation or device capabilities reject FSR.

No performance gain is claimed until tests are measured. FSR is spatial only; there is **no temporal upscaling or frame generation** here.

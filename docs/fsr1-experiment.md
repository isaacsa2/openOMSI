# FSR 1 test build (opt-in)

This branch adds AMD FidelityFX Super Resolution **1.x spatial upscaling** using EASU + RCAS, using shaders derived from AMD's MIT-licensed reference. No native driver DLLs are required.

## Enable and compare

FSR1 is **disabled by default**. Open **Settings > Graphics > Upscaling** in the launcher (or **Esc > Options > Graphics** inside the game) and choose **AMD FSR 1 (experimental)**. Choose an **FSR quality** preset (Ultra Quality 77%, Quality 67%, Balanced 59%, Performance 50%, Automatic, or Custom) and **FSR sharpening** (0-100%). Settings are persisted in `settings.cfg` and included in graphics profiles.

**In the game the change shows at once**: FSR 1's passes are compiled the first time it is chosen (nothing for whoever never does). If the graphics driver refuses them, the game says so once and keeps the standard upscaler for the rest of the run. The FSR quality row shows the size the 3D picture is drawn at and the window's (`1286×723 … 1920×1080`).

- **Automatic** leaves the size to the frame-rate governor (render scale *Auto*): the picture is drawn smaller while the card cannot keep the frame rate (85, 70, 55 %) and FSR 1 scales it up; at full size nothing is upscaled.
- For a quick 1080p test, select **Quality**; the game will use a 0.67 internal scale.
- Ultra Quality: 0.77; Balanced: ~0.59; Performance: 0.50 (the renderer clamps fixed scales to 0.5).
- FSR is used for the window, for each panel of a triple screen, and in VR (below). The unchanged Catmull-Rom upscaler is used elsewhere (mirrors, offscreen pictures) and whenever FSR is off or nothing is scaled.
- **VR:** without FSR the headset's image itself is made at the eye resolution (`vr_scale`, 65 % by default) and the runtime scales it. With FSR on when VR starts, the image is full-sized and each eye is drawn at the eye resolution and scaled up to it by FSR 1 - sharper at the same drawing cost, plus the two FSR passes per eye and a larger image for the runtime to show. Turned off during the VR session, the eyes stay at that size and the standard upscaler takes over. **Not tried on a headset yet**: the renderer's part is tested offscreen (`a_vr_eye_with_fsr1_is_drawn_smaller_and_scaled_up`).
- **Mirrors** keep their own size: they are small (256 by default) and FSR would add two passes per mirror picture for little; their GPU time shows as `mirrors: mirror` in the measurements below, which decides whether that is worth trying.
- RCAS sharpening is controlled by the FSR sharpening slider (defaults to 30%), over its whole range: 100% is AMD's strongest (its limit keeps RCAS from ringing). It used to stop at 80%, the standard upscaler's limit (`fsr1_sharpening_uses_the_whole_slider`). At 0% RCAS would give its input back, so it is skipped: EASU draws the window's picture itself, one full-screen pass and its full-size intermediate fewer (`fsr1_without_sharpening_skips_rcas`). Only `FSR1 EASU` then shows in the pass times.
- The `OMSI_FSR1=1` environment override still works for developer testing; with it the existing **Render scale** setting controls the internal resolution.

## Measuring it (send numbers, not impressions)

Compare at the same place, standing still, with MSAA, backend and window size held fixed, and the frame limit off (`max fps` 0 or high: a limit under the card's rate hides every difference, and below 50 it also stops the governor).

**Frame rate, in one run.** Start with `OMSI_PROFILE=1`: once a second the log (`game.log`) says `profile interval: 30.3 fps over 1.01 s`. Switch the upscaler or the FSR quality in Esc > Options > Graphics (no restart needed) and wait a minute between switches; the averages of each stretch compare. To compare FSR 1 with the standard upscaler *at the same internal resolution*, set the standard one's **Render scale** to the FSR quality's (67 % for Quality).

**GPU time per pass, one run per setting.** The passes' times (`OMSI_GPU_TIMERS=1`) are written once, when the game ends by itself after `--exit-after <seconds>`: `profile gpu pass FSR1 EASU: … ms`, `FSR1 RCAS`, or `upscale` for the standard upscaler, and `profile: window 1161x653, scene drawn at 778x437`. Start `openomsi.exe` with the command line the log's `command line:` names plus `--exit-after 120` (PowerShell: `$env:OMSI_PROFILE='1'; $env:OMSI_GPU_TIMERS='1'; .\openomsi.exe <that command line> --exit-after 120`), once with each setting. With `OMSI_PROFILE_JSON=<file>` the same summary goes to a file, and `scripts/compare-performance.py a.json b.json` compares two.

## Technical design

- Source picture is tone mapped, then **EASU** writes into a cached full-sized color target.
- Both passes work on a perceptual picture, as FSR 1 is made for: on an sRGB window format the texels read back linear, so they are taken to gamma 2.0 on the way in and back to linear on the way out (as AMD's sample does); on a format that is not sRGB the stored values are used as they are. Without it, EASU's edge test and RCAS's limits ran on linear light (more halos in the darks, uneven sharpening).
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
5. Verify resize, minimized/restored windows, and the runtime fallback when a driver rejects FSR (covered on the software adapter by `fsr1_turned_on_while_running_upscales_the_picture`).

No performance gain is claimed until tests are measured. FSR is spatial only; there is **no temporal upscaling or frame generation** here.

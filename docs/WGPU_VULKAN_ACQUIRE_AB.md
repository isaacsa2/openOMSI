# WGPU 29 backend fixes: controlled A/B tests

This draft combines **three isolated bug fixes** in the **same WGPU 29 fork** pinned
by upstream openOMSI at the start of the experiment. It does not upgrade WGPU or
introduce new rendering features.

- **Control:** `openOMSI-org/openOMSI` main with the WGPU 29 revision `4eb06e5bf8062be2978d93e95556a4dd83f1d3b8`.
- **Candidate:** this draft PR using [isaacsa2/wgpu-openomsi@3caafa7](https://github.com/isaacsa2/wgpu-openomsi/commit/3caafa697fea9be157c0fd481c9ecb02a546c14b).
- The candidate preserves the existing WGPU 29 game-specific patches; the only WGPU changes are in `wgpu-hal/src/vulkan/swapchain/native.rs` and `wgpu-hal/src/gles/egl.rs`.

## Included fixes

1. **Vulkan / Windows non-FIFO acquire-fence wait**, ported from
   [wgpu #10641](https://github.com/gfx-rs/wgpu/pull/10641), prompted by
   [#9559](https://github.com/gfx-rs/wgpu/issues/9559).
   Only throttle Vulkan acquire with a fence on Windows when the present mode
   is `Fifo` or `FifoRelaxed`. This avoids additional acquisition wait in
   `Immediate` and `Mailbox` while keeping FIFO frame pacing unchanged.
2. **GLES / EGL duplicated surface type attributes**, ported from upstream
   merged [wgpu #10048](https://github.com/gfx-rs/wgpu/pull/10048);
   isolated review: [fork #7](https://github.com/isaacsa2/wgpu-openomsi/pull/7).
   Combine `EGL_SURFACE_TYPE` bit flags across tiers instead of passing duplicate
   keys to `eglChooseConfig`. Relevant to EGL initialization failures on Mesa.
3. **GLES default framebuffer back-buffer selection**, ported from upstream
   merged [wgpu #10268](https://github.com/gfx-rs/wgpu/pull/10268);
   isolated review: [fork #8](https://github.com/isaacsa2/wgpu-openomsi/pull/8).
   Explicitly choose `BACK` before the presentation blit to avoid an all-black
   window on drivers that do not select that buffer automatically.

These GLES fixes address **correctness/compatibility**, not proven FPS gains.
The Vulkan fix is a latency experiment, not a validated openOMSI performance
improvement. A separate GL/ANGLE `drawing` or `submit` stall investigation
is still necessary if those stages remain slow.

## Build and test

Build **both** control and candidate with the same Rust toolchain, graphics
driver and `cargo build --release --locked -p omsi-app` command. Keep
executables in separate folders. The candidate changes all nine WGPU/Naga
`[patch.crates-io]` entries **and** the nine related `Cargo.lock` source
references; ensure the actual build uses the committed lockfile.

For **Vulkan** on Windows, use an identical visible-window scene, map, bus,
camera, weather, resolution, settings, and graphics driver:

```powershell
$env:OMSI_BACKEND = 'vulkan'
$env:OMSI_PROFILE = '1'
$env:OMSI_GPU_TIMERS = '1'
$env:OMSI_MAX_FPS = '0'
$env:OMSI_PROFILE_JSON = 'before.json' # control run
.\target\release\openomsi.exe --exit-after 60
```

Run the candidate in a separate checkout with `after.json`. Run at least
three iterations per build, alternate order to account for shader caches/
temperature, compare **V-Sync off** first and then **V-Sync on**. Track
`acquire`, `drawing`, `submit`, `present`, frame p50/p95/p99, GPU
pass times, FPS, crashes and any frame-pacing regressions.

```powershell
python scripts/compare-performance.py before.json after.json --fail-p95-percent 10
```

Do not use `OMSI_PROFILE_GPU=1` for normal FPS comparisons because it
forces synchronization. Confirm the actual Vulkan present mode if possible:
`AutoNoVsync` may select different modes on different systems.

For **OpenGL/GLES/ANGLE**, validate **startup + actual presentation** on Mesa
Linux, NVIDIA/X11, Windows ANGLE and Android where hardware is available.
Pay particular attention to EGL configuration failures and black windows.
Run both V-Sync modes and render existing OMSI maps/buses; check shaders,
materials, transparency and failure logs. The direct GLES checks should not
be represented as Vulkan FPS improvements.

## Validation and rollback

- `cargo test --workspace --locked`
- `cargo build --release --locked`
- Target-specific WGPU HAL builds, GLES validation and CI on Windows, Linux
  and Android.
- No new crashes, missing geometry, driver errors or frame-pacing regressions.
- If one fix regresses, compare against its standalone branch/PR or revert that
  specific WGPU change, then repin all nine lockfile entries.
- The reference/control remains the upstream openOMSI main; no changes are made
  to it by this experiment.

**Do not merge to upstream based on the external issues or CI compilation
alone. Hardware A/B and compatibility tests are still required.**

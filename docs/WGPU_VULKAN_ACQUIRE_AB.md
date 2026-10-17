# Windows Vulkan acquire-fence A/B test (WGPU 29)

This draft tests a single downstream WGPU change, based on `wgpu-openomsi` commit
`4eb06e5bf8062be2978d93e95556a4dd83f1d3b8` (the revision used by upstream
openOMSI at the time this branch was created).

- **Control:** `openOMSI-org/openOMSI` main, unchanged WGPU 29 fork.
- **Candidate:** this branch, with the same WGPU 29 revision plus the one-condition
  patch at [isaacsa2/wgpu-openomsi@2409547](https://github.com/isaacsa2/wgpu-openomsi/commit/2409547bccdacb323dcf65e1ec11b1c519c41f92).
- **Upstream motivation:** gfx-rs/wgpu [#9559](https://github.com/gfx-rs/wgpu/issues/9559),
  [#10641](https://github.com/gfx-rs/wgpu/pull/10641).
- **Effect:** on Windows, wait for Vulkan swapchain acquisition fence for FIFO/FIFO_RELAXED;
  skip the extra wait for IMMEDIATE/MAILBOX. No other renderer code is changed.

## Reproduce

Build **both** control and candidate using `cargo build --release --locked -p omsi-app`
and the same Rust toolchain, on the same PC and driver. Keep the two executables in
separate folders. This branch changes Git dependencies in `Cargo.toml` **and**
`Cargo.lock`; use the lockfile committed on each branch.

From PowerShell, with a **visible** window and identical map, bus, camera,
time/weather, graphics options, resolution and spawn location:

```powershell
$env:OMSI_BACKEND = 'vulkan'
$env:OMSI_PROFILE = '1'
$env:OMSI_GPU_TIMERS = '1'
$env:OMSI_MAX_FPS = '0'
$env:OMSI_PROFILE_JSON = 'before.json' # control run only
.\target\release\openomsi.exe --exit-after 60
```

Run the candidate in its own checkout, changing `OMSI_PROFILE_JSON` to
`after.json`. Repeat at least three times per build and alternate run order
to reduce temperature / shader-cache effects. Set V-Sync **OFF** for the
first comparison, then repeat with V-Sync **ON** as a control. Track:
`acquire`, `submit`, `present`, `drawing`, FPS, p50/p95/p99 frame
times, stalls, GPU time and device errors. Confirm actual present mode if
the platform exposes it; `AutoNoVsync` may choose different modes.

Compare:
```powershell
python scripts/compare-performance.py before.json after.json --fail-p95-percent 10
```

Do not use `OMSI_PROFILE_GPU=1` for normal performance comparisons (it
synchronously waits for GPU work). Also retest DX12, ANGLE and GL as
no-change controls. This fix is **Vulkan/Windows only**, and is **not**
a fix for OpenGL/ANGLE command replay in `queue.submit`.

## Acceptance criteria

- No Vulkan validation errors, crashes, missing geometry or synchronization
  failures on either V-Sync setting.
- Improved `acquire` latency or frame p95/p99 in the affected configuration,
  without regressions in FIFO/V-Sync frame pacing.
- Stable results across multiple runs and at least AMD/NVIDIA Windows hardware
  where possible.
- CI: `cargo test --workspace --locked`, `cargo build --release --locked`,
  plus platform-specific builds. **Results have not yet been measured.**

**Experimental branch only. Do not merge to the upstream project based solely on
the external WGPU issue's results.**

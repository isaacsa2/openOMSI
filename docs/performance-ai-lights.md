# AI headlights / lighting-grid performance A/B

This **experimental** branch investigates the FPS regression reported in [#3017](https://github.com/openOMSI-org/openOMSI/issues/3017), including the reporter's follow-up about Moskur AI vehicles with working low-beam headlights. It does not change vehicle scripts, turn headlights off in the simulation, alter multiplayer packets, or touch saved content.

## Changes

1. **Conservative light-grid culling (on by default):** the Enhanced headlamp shader returns zero behind a headlamp's horizontal forward axis. The CPU now omits only 25 m lighting-grid cells *entirely* behind that axis, accounting for the full cell extent. Glows/coronas, meshes, and the actual headlamp stay present. `OMSI_NO_HEADLAMP_CELL_CULL=1` restores the old grid layout for exact A/B. This option affects projected headlamp light, not headlights' model scripts.
2. **Clear-day AI spot-budget experiment (off by default):** set `OMSI_AI_DAY_SPOTS=8` to keep *projected lights* only for the eight nearest AI vehicles during bright daytime (sun brightness >= 0.9, static lamps off, visibility >= 2 km). Other AI vehicles still simulate normally, retain their visible light coronas, textures, indicators and scripts. The player's vehicle and LAN players are always exempt; night, dawn/dusk, and fog disable the budget automatically. **This setting deliberately changes daytime road lighting, and is not a shipping-quality recommendation.** Leaving it unset keeps all projected lights. `0`, `8` and `16` are useful experiments.
3. **Profiling:** with `OMSI_PROFILE=1`, the scene logs a sampled count of AI projected lights removed at most once per ten seconds while the budget is enabled. Existing CPU stage timing and optional asynchronous GPU timestamp queries remain available, without a forced `device.poll(Wait)`.

## Run the comparison (Windows PowerShell)

Use a release build of this PR. Match map, bus, spawn/entry, time, seed, API, preset, mirror settings and AI traffic density across runs. Include both the problematic Moskur traffic and a stock-traffic control. Keep VSync, automated scaling and FPS limit constant; preferably disable any limit that caps the baseline. Complete one run before starting the next. Do not compile or download during measurement.

```powershell
$exe = "C:\path\to\openomsi.exe"
$root = "D:\OMSI 2"
$env:OMSI_BACKEND = "vulkan"
$env:OMSI_PROFILE = "1"
$env:OMSI_GPU_TIMERS = "1"   # only if supported by the adapter
$env:OMSI_SEED = "54433068"
# Keep identical command-line scene args across all three runs:
$scene = @("--root", $root, "--no-menu", "--map", "maps/Grundorf/global.cfg",
           "--bus", "Vehicles/MAN_SD200/MAN_SD80.bus", "--entry", "0",
           "--time", "12:00", "--traffic", "40", "--exit-after", "150")

# A: Original lighting grid and all AI projected beams
$env:OMSI_NO_HEADLAMP_CELL_CULL = "1"
Remove-Item Env:OMSI_AI_DAY_SPOTS -ErrorAction SilentlyContinue
$env:OMSI_PROFILE_JSON = "$PWD\ai-lights-A.json"
& $exe @scene

# B: Conservative grid-cell optimization only
Remove-Item Env:OMSI_NO_HEADLAMP_CELL_CULL -ErrorAction SilentlyContinue
$env:OMSI_PROFILE_JSON = "$PWD\ai-lights-B.json"
& $exe @scene

# C: Optimized grid, eight nearest AI vehicles keep day projected beams
$env:OMSI_AI_DAY_SPOTS = "8"
$env:OMSI_PROFILE_JSON = "$PWD\ai-lights-C.json"
& $exe @scene

# Compare the existing profiler output.
python scripts/compare-performance.py ai-lights-A.json ai-lights-B.json
python scripts/compare-performance.py ai-lights-B.json ai-lights-C.json

# Remove test-only environment variables
"OMSI_NO_HEADLAMP_CELL_CULL","OMSI_AI_DAY_SPOTS","OMSI_PROFILE_JSON",
"OMSI_PROFILE","OMSI_GPU_TIMERS","OMSI_SEED","OMSI_BACKEND" |
    ForEach-Object { Remove-Item "Env:$_" -ErrorAction SilentlyContinue }
```

Replace the sample map, bus, entry and AI count with an **actual tested scene**. The sample is not the reporter's configuration and is not evidence of the regression. Repeat at 09:00 and 21:00, and in heavy rain/fog; compare Enhanced/Vulkan and Enhanced/DX12. At night and in fog, run C should preserve every AI projected light, but cell culling can still optimize headlamps.

**Success criteria:** improved median FPS and p95/p99 without visible missing light on nearby road/buildings, no headlamp glow or script regression, and no performance regression in Vanilla/Vanilla+. Check GPU pass timing and CPU `LightsCollect`; separate preloading from steady-state frames. Use screenshots from a deterministic camera for the A/B review.

**Do not enable `OMSI_PROFILE_GPU=1` for representative frame timing:** that flag explicitly waits for the GPU and changes scheduling. Do not claim an FPS improvement until A/B numbers are attached.

## Validation

- Unit tests: headlamp cell bounds (forward, behind, diagonal, cell boundary, non-headlamp); daytime gating (unset, night, twilight, fog).
- `cargo test -p omsi-render -p omsi-scene -p omsi-cfg` and `cargo build --release -p omsi-app`.
- Manual: an AI bus with a scripted low beam during daylight; same spot in twilight, rainy weather and night; driver and external view; near/far and oblique view angles; other LAN player headlights.
- Compare outputs only from identical scene/preset/backend/hardware, and leave this PR as Draft until the results are recorded.

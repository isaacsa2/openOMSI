# Bounded profiler capture

## Built-in stock benchmark

For repeatable version-to-version measurements, use the built-in stock-content benchmark:

```sh
openomsi --root "<OMSI 2 folder>" --benchmark
```

The launcher exposes the same run under **Setup → Run performance benchmark**.

The current scenario is tagged `stock-grundorf-v1` in `performance.json`. It uses only
content from a complete original OMSI 2 installation:

- map: `maps/Grundorf/global.cfg`;
- player bus: `Vehicles/MAN_SD200/MAN_SD80.bus`;
- Grundorf's stock AI list and timetable, with 20 random traffic vehicles;
- passengers and scheduled AI buses enabled;
- 09:00 on 1989-05-30, summer, natural weather;
- driver view at entry point 0;
- fixed session/population seed `30794024`;
- 1600×900 window, V-sync off, 100% fixed render scale and no frame cap;
- 20 seconds of warm-up followed by a 30-second capture by default.

The selected graphics mode and graphics backend are deliberately **not** replaced. This lets
the same scenario measure Vanilla, Vanilla+, Enhanced or Enhanced+ (and Vulkan/DX12/etc.)
without pretending those different settings are directly comparable. The capture records the
effective graphics settings so the comparison tool rejects accidental mismatches.

The benchmark does not modify `settings.cfg`; its fixed pacing, size and sync overrides are
local to that run. It also seeds traffic and passengers, not just vehicle scripts.

The default result folder is
`<openOMSI data>/benchmark-stock-grundorf-v1-<pid>/`, containing
`performance.json` and `performance-summary.txt`.

To compare two builds on the same machine:

```sh
python scripts/compare-performance.py before/performance.json after/performance.json
```

The benchmark is intended primarily for A/B regression testing on the same hardware. Different
GPUs, drivers, CPUs or graphics presets can be interesting hardware measurements, but they are
not interchangeable regression baselines.

Use the same command line as a reproducible session, adding:

```sh
openomsi --root "<OMSI folder>" --no-menu --map maps/Grundorf/global.cfg --profile-capture 30 --profile-output capture-1
```

Durations: `10`, `30`, `60` seconds. Capture waits for the world to load, then
warms up for 15 seconds (`--profile-delay 0` includes the next loaded frames).
Keep camera, map/bus, traffic, passengers, time, settings and warm-up identical
when comparing. Do not compare captures from distinct presets as a fix claim.

This enables OMSI_PROFILE through a thread-safe override, without changing the
process environment. It exports existing cumulative app/renderer CPU stages as
per-frame deltas, then restores ordinary profiling when complete. No GPU query
feature is forced: GPU aggregates are included only if existing timestamp timers
were enabled and supported (`OMSI_GPU_TIMERS`). An empty GPU list means unknown.
`gpu` is CPU time waiting on the GPU, not GPU execution time. GPU pass samples
are asynchronous aggregates and may cover fewer frames than the CPU capture.

Files: `performance-summary.txt` and `performance.json`. Existing captures are
never overwritten. Without `--profile-output`, the application data folder holds
`performance-<pid>`. Export happens on a worker thread after sampling ends.
Records contain frame number, elapsed timestamp, wall frame time, inclusive
stage timings, FPS from total measured time, nearest-rank p50/p95/p99/worst,
threshold counts and start/end memory. Do not sum parent stages and sub-stages.
No settings dump, environment, command line, paths, player data or hardware
serials are exported. The frame limit (120,000) is reported if reached.

Current limitation: exiting or losing the device before the duration completes
does not produce a partial capture. No frame samples are kept while disabled;
the bounded override is restored at completion. Existing OMSI_PROFILE remains
enabled if the user set it independently.

Manual tests: 10/30/60 s, no timestamps/OpenGL, Vulkan timestamps already enabled,
paused/hidden window (annotate the report), fallback resetting renderer counters,
LAN active/inactive, same scene looking at windshield vs roof (#1710), MSAA 8x
vs 1x as a diagnostic comparison, and before the SSAO governor triggers (#1716).
Use the support-package change separately for hardware/graphics metadata.

Run `cargo test --workspace` and `cargo build --release`. This change makes FPS
issues measurable; it does not claim to fix #1091/#1710/#1716.

# Non-blocking performance profiling

This reference documents how to measure FPS regressions in normal, windowed
openOMSI runs. Profiling adds some overhead, but this GPU timestamp path
does **not** deliberately wait for the GPU to finish each frame.

## What is measured

- OMSI_PROFILE: CPU frame-stage, encoding, submission and frame-time
  statistics (p50/p95/p99/worst and stutter/evenness).
- OMSI_GPU_TIMERS: per-pass GPU timestamps when the device supports
  TIMESTAMP_QUERY, with mirrors and the window tracked separately.
- OMSI_PROFILE_JSON: JSON summary written on exit when OMSI_PROFILE
  and --exit-after are also set. Warm-up: 15 seconds after map load.
- render.timer_readback: CPU wall time for processing available GPU
  results and submitting query resolves/copies, not GPU execution.
- gpu_timer_coverage: window/mirror counts for submitted, resolved,
  skipped_busy and readback_failed (includes warm-up).

### Async sampling and its limits

Each image type has **three rotating timestamp slots**. A free slot is
sampled, its query results are resolved, then copied and read with
map_async. Readback callbacks are checked without blocking the render
thread. If all slots are busy, the profiler skips a sample rather than
forcing synchronization. Coverage counters make this visible.

Do **not** enable OMSI_PROFILE_GPU when comparing gameplay FPS:
that separate legacy option explicitly calls wait_gpu() inside the
render path and changes CPU/GPU overlap. Boolean OMSI_* flags are
enabled by their mere presence, so setting OMSI_PROFILE_GPU=0
**still enables it**. Unset it completely.

GPU pass statistics are sampled averages; they are not guaranteed to
cover every frame. Untimed work between GPU passes can be attributed
to the following timed pass. CPU and GPU stage durations may overlap;
do not simply add them together. The instrumented timestamp queries
still have some cost, and are absent on unsupported GPU backends.
Existing presentation synchronization and device recovery are unchanged.

## PowerShell example (Windows)

Run from the directory containing openomsi.exe; adjust map and bus to
match your OMSI 2 installation:

~~~powershell
Remove-Item Env:OMSI_PROFILE_GPU -ErrorAction SilentlyContinue
$env:OMSI_PROFILE = "1"
$env:OMSI_GPU_TIMERS = "1"
$env:OMSI_PROFILE_JSON = "$PWD\benchmark.json"
$env:RUST_LOG = "info"
.\openomsi.exe --root "D:\Games\OMSI 2" --no-menu --map "maps/Grundorf/global.cfg" --bus "Vehicles/MAN_SD200/MAN_SD80.bus" --view driver --time 09:00 --traffic 20 --schedule --passengers --exit-after 120
~~~

This is a **normal window** run, not an offscreen render. The session
must start gameplay and exit normally to create its JSON file.

## Comparing performance

1. Keep GPU driver, graphics backend, map, bus, camera, resolution,
   time, weather, MSAA, preset, traffic, passengers, mirrors and
   V-sync identical between builds.
2. Allow content/shaders to warm up and use the same timed duration.
   Prefer multiple runs rather than a single average FPS.
3. Compare p95/p99 frame times, stutters and per-stage costs; also
   check gpu_timer_coverage before trusting GPU averages.
4. Use the existing analysis script:

~~~shell
python scripts/compare-performance.py before.json after.json
~~~

A high acquire/present value may reflect a display or compositor wait;
a high submit value may reflect graphics driver command replay. Neither
by itself proves a shader bug or GPU saturation.

The in-game performance overlay can be set to detailed in Settings;
the more complete developer flag inventory is in DEBUG_FLAGS.md.
For safe variables see ENVIRONMENT_REFERENCE.md and for launch
commands see COMMAND_REFERENCE.md.

A dedicated server has no windowed rendering device; use its normal
server logs and world counts rather than GPU timestamp queries.

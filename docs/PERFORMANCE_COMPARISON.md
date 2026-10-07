# Performance capture comparison

`scripts/compare-performance.py` compares two schema-1 captures produced by the
bounded profiler capture. It does not run the game and it does not turn a noisy
real-time workload into a deterministic benchmark.

Use it only for A/B runs made on the same machine with the same map, bus, camera,
time, traffic/passenger density, graphics backend and graphics settings.

```sh
python scripts/compare-performance.py before/performance.json after/performance.json
```

The report shows average FPS, p50/p95/p99/worst frame time and the largest changes
in CPU stage averages. Capture duration, warm-up and the recorded graphics state
are compared too; mismatches make the command fail unless `--allow-mismatch` is
given deliberately.

For experiments where a threshold is useful:

```sh
python scripts/compare-performance.py before/performance.json after/performance.json \
  --fail-p95-percent 10 \
  --fail-p99-percent 15 \
  --fail-fps-drop-percent 10
```

Exit status is 0 when the comparison is valid and no requested threshold is
exceeded, 1 for a metadata mismatch or exceeded threshold, and 2 for invalid
input/arguments. `--json` emits a machine-readable comparison for CI or other
tools.

## What this can and cannot prove

A same-scene A/B run can locate a regression and show which measured stages grew.
It cannot make results from different GPUs, drivers, maps, cameras or graphics
presets directly comparable. Traffic and background work are still variable, so
repeat a suspicious result and keep the raw captures.

Do not use a single FPS average as the only gate. p95/p99 frame time and the
specific stages usually make stutter/regression changes clearer.

A future CI job may consume this tool when a redistributable deterministic scene
exists. The repository must not add OMSI 2 or paid DLC assets merely to create a
benchmark.

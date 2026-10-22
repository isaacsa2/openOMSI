# Investigating AI-traffic lifetime and vehicle-cache residency

This procedure distinguishes **stuck traffic**, **entity churn**, **timetable work** and
**retained GPU vehicle sets**. It does not change AI despawn or spawn behaviour.

## What is counted

Set `OMSI_TRAFFIC_STATS` to write one CSV row per 60 seconds of **simulation time**.
Existing columns remain in their original order. The added columns, after `heads`, are:

| Column | Meaning |
| --- | --- |
| `buses` | Mean number of active timetable buses sampled once per simulation second in the window |
| `removed_dormant` | Active cars converted to cheap dormant traffic in this window |
| `removed_unloaded` | Active vehicles removed when the underlying map/route was unloaded |
| `removed_finished` | Active vehicles removed after finishing, parking or giving up |
| `removed_taken_over` | Active vehicles removed when taken over by the player or timetable |
| `removed_invalid_route` | Active vehicles removed because the route became invalid |

The removal counters are **per-window deltas**, not cumulative. Conversion to
`DormantCar` does not mean a car was destroyed: the car still exists as a
lightweight record. `gone` means the car has given up but is still in the
active registry. `gave_up` counts the decision to give up, which can occur
before its eventual removal. A car can be removed without first giving up.

With `OMSI_PROFILE=1`, each vehicle-set trimming pass now also reports:
- **active sets**: shared GPU sets with one or more users;
- **idle pinned sets**: no active users, but preserved by the keep-list
  (including timetable prefetch and random traffic types/paint schemes);
- **idle unpinned sets**: no users and not protected; they may still be
  inside the 90-second default grace period;
- **due for eviction**: idle, unprotected sets old enough to be removed now.

These are **counts of sets**, not bytes, GPU allocation totals or proof of a
leak. The profile logging only inspects CPU-side bookkeeping and does not
wait for a GPU timestamp or force readback. Fleet trimming runs when the
timetable's fleet maintenance path runs, so the absence of these log
messages does not by itself mean zero idle sets.

## Recording on Windows

Configure these variables before launching the game (PowerShell):

```powershell
$env:OMSI_PROFILE = "1"
$env:OMSI_TRAFFIC_STATS = "traffic-lifetime.csv"
$env:OMSI_SEED = "1"
# Optional for a short diagnostic run. This can produce verbose logs:
$env:OMSI_DEBUG_POPULATION = "1"
# Optional only while diagnosing a specific traffic jam:
# $env:OMSI_DEBUG_STUCK = "1"
```

Save the game log alongside the CSV and note the build SHA, map, vehicle,
camera, graphics API, preset, seed, spawn density, schedule, game date/time,
weather, and LAN mode. Also record the **process private commit**, working
set and system commit as separate quantities; note GPU dedicated-memory
usage separately from the game's texture/mesh counters.

Avoid `OMSI_PROFILE_GPU` during the first comparison: GPU profiling can
force waits and change the timing being investigated.

## Comparison matrix

1. **Normal traffic:** run a representative 30-minute route with random
   traffic and timetable buses enabled.
2. **No random cars:** preserve the same timetable, route and settings.
3. **No timetable buses:** preserve random traffic and all other settings.
4. **Recovery experiment:** in a normal run, note counts and timings
   immediately before and after using the game's **Clear AI traffic**
   control. It removes random active cars but keeps scheduled buses; the
   configured traffic target remains unchanged, so cars may spawn again.

Repeat with matching seeds and multiple runs; compare equivalent route
segments, not just averages of different locations. The first minutes
include load/warm-up effects. Capture median, p95 and p99 frame time, and
`traffic.tick`, `traffic.populate`, `traffic.schedule` stage times.

## Interpreting evidence

- If active car counts steadily exceed the configured target while
  `removed_*` stays low, investigate population counting and admission.
- If active counts remain stable but `removed_dormant` and
  `removed_finished` are high, investigate spawn/despawn churn and
  repeated GPU creation/release.
- If `buses` or waiting-chain metrics grow, distinguish schedule hold,
  traffic-light wait, physical gridlock and broken paths before despawning.
- If FPS improves after clearing traffic but idle GPU sets remain pinned,
  separate **per-vehicle runtime cost** from **cached GPU resources**.
- If memory rises while counts and GPU sets remain steady, inspect other
  caches/allocators: traffic is not yet the established cause.

Do not shorten the 200-second random-traffic give-up threshold or the
scheduled-bus safeguards based on an FPS report alone. Removing a bus
prematurely can break stops, passengers and timetables.

## Code entry points

- `crates/omsi-sim/src/ai_traffic/stats.rs`: CSV counts and removal deltas.
- `crates/omsi-sim/src/ai_traffic/tick.rs`: driving, waiting and give-up.
- `crates/omsi-app/src/traffic/population.rs`: admission, sleep and despawn.
- `crates/omsi-app/src/traffic/control.rs`: clear random traffic.
- `crates/omsi-app/src/schedule/fleet.rs`: keep-list and fleet maintenance.
- `crates/omsi-scene/src/vehicles.rs`: shared resource usage and eviction.

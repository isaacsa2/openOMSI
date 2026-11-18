# Dedicated openOMSI server — regression acceptance matrix

This document defines **release gates**, not a claim that all gates have passed.
The new core must not replace `openomsi --server` until every required scenario
below has been checked on a clean Linux machine without a display or GPU.

## Automated gates

| Gate | Verification | Required result |
|---|---|---|
| Rust workspace | `cargo test --locked --workspace --no-fail-fast` | No Rust test regressions |
| Dedicated server | `cargo test --locked -p omsi-server --all-targets` | Two-client relay, no phantom host bus, tick/profiler, CPU cache, mirrored lanes |
| Standalone CPU loaders | `cargo build --locked -p omsi-server --bins` | `worldcheck` and `simcheck` link without `omsi-render`, `omsi-scene`, `wgpu`, `winit` or `omsi-audio` |
| Runtime dependency graph | `cargo tree --locked -p omsi-server --edges normal` | No graphical or audio runtime dependencies |
| Style and architecture | Existing PR checks | Formatting, file limits, crate-layering and app-growth guards pass |
| Safety | Existing audit, CodeQL and secret scanning | All checks pass |

The network test spins up a real UDP/TCP host on an ephemeral port and two
ordinary openOMSI client sessions. It asserts the clients' vehicles propagate,
they receive the same map/date/weather and the server itself owns no bus. It
does **not** replace two separate game clients driving a licensed map.

## Licensed-map CPU smoke and load measurements

```sh
cargo run --locked -p omsi-server --bin simcheck -- /path/to/OMSI2 Maps/Grundorf/global.cfg 19890501 600 16
```

This bounded, standalone command reads a real map's initial tiles, loads
`ailists.cfg` AI vehicle types and road path metadata, then advances the
production traffic and passenger simulation for 600 fixed ticks. It reports
cold-load duration, sampled tick latency and **unpaced offline throughput**.
No GPU, audio or multiplayer socket is used. The selected first tiles may not
represent a populated route and traffic target is deliberately zero while
road collision and schedules are still being migrated. This is **not** a
claim of 60 live TPS, passenger correctness or playable multiplayer.

## Manual end-to-end acceptance gates — blocking

1. **No GPU/no display:** on a clean Linux VPS without X11, Wayland, Vulkan,
   DirectX or an audio service, launch the final native `omsi-server` process.
   Confirm zero graphics context initialization. `worldcheck` alone does not
   satisfy this acceptance gate.
2. **Map correctness:** test Grundorf, Berlin-Spandau, a Chrono map and a third
   party map. Compare world coordinates, terrain, crossing/road collision,
   traffic lights, depots, stops and timetable routes with the baseline host.
3. **Vehicles:** two genuine clients enter different buses, drive, change
   liveries, operate doors, scripts and destination displays; confirm state
   agrees at both clients and a new joiner receives complete snapshots.
4. **AI:** compare random/scheduled traffic counts and movement, one-way and
   mirrored roads, intersection right-of-way, rail signals and pedestrian
   crossings. Unloaded distant tiles must not corrupt timetables or AI state.
5. **Passengers:** loading/waiting/boarding/alighting and connection timing
   must match the current server and remain correct across client joins.
6. **Security:** verify kicks, bans, hardware bans, authentication, moderator
   rights, `share_maps = 0`, upload restrictions, protected/private bus files
   and no unauthorized transfers. Never expose a command shell or RCON.
7. **Reliability:** restart and rejoin, client crash/reconnect, bad packets,
   invalid files, host clock change, METAR timeout and headless shutdown.
8. **Performance:** for equal map, script count, AI, weather and number of
   players, record 10 minutes of warm and steady-state TPS, p95/p99 tick
   latency, busiest core CPU%, process RSS/peak RSS and disk I/O. Compare to
   old `openomsi --server`. No speedup claim without measured data.
9. **Distribution:** compare Linux/Windows headless runtime binary and
   dependency size. Keep only permitted OMSI 2 assets and required map/script
   files; never redistribute private vehicles, textures or paid maps.

## Release policy

Any incomplete gate means the PR remains **Draft**. In particular, a
renderer-free `worldcheck` diagnostic, a CPU lane parser, or a connected host
without authoritative bus physics and passenger simulation is **not a
finished dedicated server**. Do not point Docker/production at a diagnostic
binary. Keep the old host working until the new runtime passes all gates.

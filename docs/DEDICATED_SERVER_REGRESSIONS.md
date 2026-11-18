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

## Partial CPU static collision and traffic light migration

The headless map loader can publish static `[boundingbox]` obstacles into
`WorldData::lists.collision`, using the map's world placement, while
avoiding oversized road-through boxes that would create invisible walls.
`simcheck` reports the remaining collision meshes, attached objects and
missing definitions. **It is not yet complete collision parity**: exact
`[collision_mesh]` triangles, road/crossing surfaces, attached/parked objects
and deformable terrain still require the dedicated CPU placement path.

The CPU lane loader can now assign `[use_traffic_light]` indices to
crossing-owned `TrafficLightController` programs, retaining the original
stop/jump phase rules and stable object IDs. The diagnostic assembles these
controllers into `TrafficSim`. Script-driven signal lamps, parented objects,
rail interlocks and timetable integration remain required acceptance tests.

These are **simulation foundations**, not authorization to call
`HostTransport::ready()` for a real map. Missing or unsupported collision
and signal records must stay visible as unresolved, not be silently converted
into boxes, ignored signals or a false-ready server.

## Experimental independent host process

The first **native renderer-free** `omsi-server` executable now exists. It
has a 60 Hz loop, actual network relay, restricted admin/console, CPU-only map
and road data, collision-box checks, traffic-light initialization, and CPU
simulation ticks. **This does not constitute full OMSI server parity**:
vehicle script ownership, full collisions, scheduled AI traffic and passenger
snapshot transfer are still incomplete.

For an **isolated test only**, configure `server.cfg` with:
`date = 1989-05-30`, `traffic = 0`, `timetable = 0`,
`passengers = 0`, `share_maps = 0`, `accept_uploads = 0`,
`parked_buses = 0`, `public_server = 0`, `tunnel = 0`, `public_address = off`,
`real_time = 0` and `metar_sync = 0`. Other values fail closed before
listening. The server never executes a remote shell and does not advertise
itself publicly.

```sh
cargo run --locked -p omsi-server --bin omsi-server -- /path/to/OMSI2 server.cfg --experimental-free-roam
```

The `--ticks 600` option bounds a diagnostic run to 600 ticks.
Without `--experimental-free-roam`, the native binary deliberately
**refuses multiplayer admission**, keeping production with `openomsi --server`.
Users must not treat a successful relay connection as proof of vehicle,
map, timetable, or passenger authority.

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

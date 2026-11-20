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

## Isolated experimental CPU AI, passengers and schedules

The **separate** `--experimental-simulation` switch activates the shared
engine's AI road-car physics and scripts, pedestrian population, existing
`TrafficTable` and `WORLD` / `DESC` serializers, and the native
`ScheduleSim` dispatcher for simple road-bus schedules. See
`scripts/server-headless/server-simulation.cfg`. The normal
`--experimental-free-roam` path remains traffic/passenger/timetable-free.

Admission still fails closed on public server discovery, tunnel, file/map
sharing, vehicle transfer, voice and unauthorized teleport. The AI population
budget is limited to 32, the human pool to 128 and the scheduled road-bus
budget to 12. Tests must verify both flags independently and confirm that
neither may enable public hosting; file sharing requires the explicit,
restart-only, per-option consent gates and the authenticated LAN mod service.

The first CPU-simulation path has **fixed initial tiles** instead of live
192-tile swaps: populated AI and pedestrian lane indices cannot be rewritten
without entity-aware migration. It does **not** establish authoritative
player-driven bus input/physics or whole-map, train, articulated consist,
real-time performance, timetable and complete collision parity.

Required external tests: a licensed installed map with usable AI vehicle and
human definitions, actual Chrono timetable content, multiple normal graphical
clients, coordinated stop boarding/CLAIMs, AI near junctions and signals, TPS
under load, late joining, reconnection, server shutdown, and client behavior
outside the initial CPU area. Do **not** mark production-ready if the CI
smoke test alone passes: a synthetic test without installed AI types cannot
prove a populated real-world scene.

## Integration status before Phase 2 review

Phase 1 connects CPU AI spawning, the timetable dispatcher, pedestrian
population, authenticated traffic table, rotating scheduled-bus script
context, world-frame pedestrian/signal snapshots and the local `/reload`
console. The native simulation remains explicitly opt-in and keeps the
legacy graphical dedicated host intact.

The Phase 2 review must include two real graphical clients and a licensed
map with installed AI scripts, road-bus HOF and Chrono data. In particular,
verify actual script context on both clients, ticketing and passenger claims,
time acceleration, a midnight/day rollover, live-reloaded traffic targets
(0 to positive and back), late-join AI ID reuse, reconnects, and duplicate
passenger prevention. The experimental native host now validates and forwards
bounded client-owned `WORLDUP`/`DESCUP` pedestrian updates using isolated IDs.
The synthetic two-client UDP regression covers their relay and removal, but
does not establish full boarding, ticketing or graphical-client parity.
Never mark physics, scripts, or passenger parity complete from synthetic
process tests alone.

## Live configuration reload regression gates

The renderer-free dedicated host accepts `/reload` from the **local
terminal only**; the stdin reader sets a one-shot flag, and the simulation
thread reads the original config filename and validates every change before
applying anything. Automated checks require:

- Valid hot changes to `traffic`, `time_speed`, `passenger_density`,
  `ai_max_scheduled` and `speed_limit_kmh` are accepted.
- Invalid numbers, deleted config, changes to map/ports/auth/transfer/public
  settings, and attempts to enable AI through free-roam are all rejected.
- Rejected reloads do not mutate the live config, traffic/scheduled limits,
  connection endpoints or policies. No user-controlled remote `reload`
  action is available.
- Lowering `traffic` retires excess unscheduled AI cars; currently active
  timetable buses may finish their runs. The session and existing clients
  remain connected.
- A real interactive-console test must verify the command produces an
  applied/unchanged/rejected status in the logs.

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

The headless map loader can publish static `[boundingbox]` and decoded `[collision_mesh]` obstacles into
`WorldData::lists.collision`, using the map's world placement, while
avoiding oversized road-through boxes that would create invisible walls. Exact
collision meshes use the existing O3D loader and simulation MeshShape, with a
bounded cache and rejection of corrupt/oversized files.
`simcheck` reports the remaining collision meshes, attached objects and
missing definitions. **It is not yet complete collision parity**: tilted mesh placements,
road/crossing surfaces, attached/parked objects
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

## Partial CPU spline road/wheel surfaces

The renderer-free host now builds the production spline triangle meshes and
uses the same `TileSurface` raster and `DriveGrid` wheel-probe logic for its
active tiles. The loader includes splines in the neighboring eight tiles so
a road crossing a tile boundary does not end at the `.map` boundary. It
preserves the `OMSI_HEIGHTPROFILE_GROUND` switch, alpha-only overlays and
the overhead-spline exclusion rules. Tests exercise triangle-height sampling
and road geometry overlapping two adjacent tiles.

Static scenery `[surface]` objects can now contribute their first CPU
O3D mesh to the road raster and wheel-probe grid, with proper world rotation
and a bounded mesh-definition cache. The loader rejects animated, missing,
corrupted and unusually large surface meshes, and leaves terrain-deformed
crossings and explicit cutter-backed geometry pending rather than placing
incorrect flat surfaces.

**Not yet parity:** the native executable starts with up to 64
entrypoint-oriented tiles. Restricted free-roam now chooses additional tiles
around connected players (up to 192 active tiles) and rebuilds CPU road lanes,
surfaces, collisions, stops and traffic-light programs on a background map
worker. The UDP networking loop continues ticking while this work runs.
Replaced regions release inactive road rasters and terrain outside the active
neighborhood; current signal clocks are carried across the lane-network swap.

This **free-roam** streaming path is experimental: it keeps AI and
pedestrian population disabled because active lane/ground indices cannot
yet migrate on unload. The separate `--experimental-simulation` mode has
bounded native AI and passenger snapshots plus timed road buses on a fixed
initial map. Neither mode has entity-aware unloading, crossing-height
deformation, terrain-hole parity, client-owned passenger relaying, or
authoritative **player-driven** bus scripts. Rejected
scenery surfaces, invalid splines and missing definitions remain visible
as unresolved counts; the road mesh is not proof of full collision parity.

## Native boot smoke fixture

A new all-targets integration test creates a small synthetic OMSI map with a
single CPU spline and launches the actual native server executable for three
fixed ticks. It deliberately removes DISPLAY/WAYLAND_DISPLAY, uses no real
OMSI map or texture files and checks clean shutdown. This catches missing
binary packaging and bootstrap dependencies missed by library tests, but
**does not replace** a licensed real-world map and two game clients.

An additional cross-process regression launches that same native executable
without a display and connects two independent production-protocol LAN
sessions through its actual UDP port. It verifies both clients receive each
other's vehicle pose, the dedicated host adds no local bus, and a departed
client frees its slot for a replacement. This is synthetic wire-level
coverage, **not** a substitute for two graphical game clients or complete
server-side vehicle authority.

## Experimental independent host process

The Release CI creates **separate** headless-test ZIPs for Windows x64 and
Linux. These contain `omsi-server`, both `server-test.cfg` and
`server-simulation.cfg`, separate `run-test` and `run-simulation`
launchers, and `README.md`. The existing `server-windows` and `server-linux` ZIPs
remain the original `openomsi --server` build. The native test package is
not promoted as a supported production host.


The first **native renderer-free** `omsi-server` executable now exists. It
has a 60 Hz loop, actual network relay, restricted admin/console, CPU-only map
and road data, collision-box checks, traffic-light initialization, and CPU
simulation ticks. **This does not constitute full OMSI server parity**:
player-driven vehicle script ownership, full collisions, client-owned rider
replication and timetable/AI parity are still incomplete. The native
`TrafficTable` plus rotating AI-context pages send scripted line,
destination, HOF stop and delay data using the same LAN encoder as the
graphical host; this requires real-client acceptance testing.

For the **isolated free-roam relay only**, configure `server.cfg` with:
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

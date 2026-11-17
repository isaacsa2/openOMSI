# Dedicated server: from a game host to a headless process

This is an implementation plan, not a claim that a standalone headless server already ships.
Keep the existing `openomsi --server server.cfg` command, packages and Docker images
working while the host is extracted. No protocol bump is required by the first stage.

## Current coupling

The server is a multiplayer host without a local driver. It runs the
`omsi-app::offscreen` path and creates a wgpu no-op renderer and a Scene.
`offscreen::setup` loads the world, AI traffic, timetable, people, weather,
remote vehicles and administration; `offscreen::step` advances them.

The `omsi-sim` crate has no rendering/network dependency, but the adapters that
populate traffic, create remote vehicles and synchronize their view are still in
`omsi-app`. `omsi-lan::game` already uses a `LanHost` trait for part of this
boundary. Splitting the executable without relocating these adapters would only
rename a graphical application, not create a dedicated server.

## Migration stages

1. **Distinct host tick (this PR).** A dedicated session takes `SERVER_STEP`
   rather than the offscreen capture/test stage list. It keeps the server
   housekeeping, clock, traffic, ground diagnostics, people, LAN and environmental
   simulation hooks, but no longer executes the local-driver/duty, screenshots or
   film-recording stages. The recorder is not constructed in server mode, and
   renderer-only road-snow tracks are not updated. The renderer and scene are
   still allocated: this step does **not** remove wgpu.
2. **Simulation-first world loading.** Extract a host-world loader with no
   `Renderer`, `Scene`, font atlas, texture upload or camera requirement.
   Move AI population, timetable departures and passenger simulation into
   renderer-independent host adapters. Make scenery and bus scripts that need
   text/image hooks use explicit optional host-side backends.
3. **Host multiplayer adapter.** Move server-side network tick and remote
   vehicle handling behind a `DedicatedLanHost` implementation in the network
   / simulation layer. Avoid calling `view_sync`, mesh placement, sound engines
   or any render-side state from this implementation. Preserve the existing
   host-authoritative clock, AI snapshots, positions, chat, content policy,
   vehicle fingerprints, voice relay and admin authorization.
4. **Standalone executable.** Add an `omsi-server` crate/binary whose runtime
   depends on `omsi-sim`, world/content, `omsi-net`, `omsi-lan`, admin and
   platform services, but not `omsi-app`, `omsi-render`, `wgpu`, `winit`,
   `omsi-scene`, `omsi-audio` or graphics/audio native libraries. The process
   owns its fixed timestep, wall-clock pacing, graceful shutdown and state
   persistence. Only after it runs a real map and accepts existing clients
   should Docker/release packaging switch to it.
5. **Compatibility and operations.** Keep the same `server.cfg` keys and
   default security policy. Maintain dedicated-server log/status endpoints,
   admin panel and authentication, public listing, ban/audit storage, tunnels,
   map/vehicle transfer controls and restore after restart. Retain
   `openomsi --server` as a compatibility entry point until the standalone
   binary has passed real-world tests.

## Non-negotiable invariants

- One authority for world time, traffic, passenger state and vehicle ownership;
  clients must not create additional AI/passengers on top of host snapshots.
- The host never needs a GPU, display server, audio device or a dummy GPU device.
- Old clients remain compatible unless an intentional wire-format change is
  documented and coordinated through `omsi-net::PROTOCOL`.
- Never weaken existing upload authorization, private-asset checks, file
  fingerprinting, limits, ban policy, or `share_maps` default.
- Shutdown stops accepting joins, flushes records, releases sockets and exits
  cleanly on SIGINT/SIGTERM; a crashed run does not corrupt persisted state.
- Server metrics distinguish connected players, simulated AI, network traffic,
  memory and tick latency. Player addresses and positions remain subject to
  their current privacy settings.

## Validation required before switching release artifacts

- Unit tests for host ticking, time-speed changes, map reload/startup errors,
  player join/leave, admin, transfer policy, AI and passenger lifecycle.
- Two clients on one dedicated server: both see each other's vehicles, paint,
  doors, passengers, traffic and timetable positions with the same world clock.
- A server on a Linux VPS with **no display server and no GPU libraries**,
  plus a Windows service/console run and Docker health check.
- Shutdown/restart with existing `server.cfg` and persistent ban/upload store.
- CPU, resident-memory and p95/p99 tick latency measured against the current
  `--server` on the same map, traffic settings and player count.

## Local checks for this stage

```sh
cargo test --locked -p omsi-app frame_stages::tests
cargo test --locked -p omsi-lan
cargo test --locked -p omsi-net
cargo build --locked -p omsi-app
```

The no-op renderer is still used by the existing entry point. Do not describe
this stage as GPU-independent or as a completed server extraction.

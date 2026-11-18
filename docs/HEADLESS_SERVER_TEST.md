# openOMSI — native dedicated server test package

**EXPERIMENTAL FREE-ROAM ONLY. Not a replacement for `openomsi --server`.**
This binary is built from PR #405 and does not initialize the graphical
renderer, DirectX, Vulkan, or audio. It uses the normal openOMSI UDP session
transport with a fixed CPU-side tick and a local server console.

## First test on Windows (including a Windows VPS)

1. Extract this ZIP to a separate directory. Keep your existing server
   installation untouched.
2. Install or locate your own licensed OMSI 2 content and verify
   `Maps/Grundorf/global.cfg` exists. Every client must already have the map
   and the same buses; the test host **never uploads or redistributes files**.
3. Double-click `run-test.cmd` (or supply the OMSI 2 directory as its first
   argument). Enter the complete OMSI 2 folder, for example
   `C:\Program Files (x86)\Steam\steamapps\common\OMSI 2`.
4. Look for the console's UDP port, server status and connection code. Use the
   matching openOMSI PR/release protocol on two normal clients. Connect by the
   code or a reachable direct address; if on another network, the VPS/router
   must permit UDP **27015**, and for WebSocket access TCP **27025**.
5. Use `list`, `tps`, `status`, `timings on`, `timings report` and
   `stop` in the server console. Profile data is collected only after
   `timings on`; it now separates `network`, `interest`, `simulation` and
   `world_sync` execution times. These are stage wall-clock times, not
   OS process CPU percentages.

## Linux test / headless VPS

Extract the native Linux ZIP, then run:

```sh
chmod +x ./omsi-server ./run-test.sh
./run-test.sh /path/to/OMSI2
```

The process does not require an X11/Wayland display or GPU runtime. All
required content remains local. To run from a source checkout instead:

```sh
cargo run --locked --release -p omsi-server --bin omsi-server -- /path/to/OMSI2 scripts/server-headless/server-test.cfg --experimental-free-roam
```

The sample configuration requires `traffic=0`, `passengers=0`,
`timetable=0`, `accept_uploads=0`, `share_maps=0`,
`public_server=0`, `public_address=off`, `tunnel=0`, a fixed date,
and related restrictions. The native test host refuses to start with
unsafe values. To use another installed map, change only the `map`
entry initially and ensure it contains usable paths.

**Important:** `public_address=off` prevents automatic address advertisement;
it is **not a firewall**. Restrict your server's UDP/TCP ports to trusted
testers using your host/VPS firewall. Do not post the join code publicly.

For a local web admin/TPS panel, set a unique `admin_password` of at least
12 characters in your private `server-test.cfg` and leave
`admin_web=local`. Never publish or commit this password. Open the local
admin URL `http://127.0.0.1:27025/admin/` on the server itself. With a blank
password, web admin is intentionally disabled.

## Streaming-test addendum: drive beyond the starting area

The native experimental host initially loads 64 tiles around the map's
entrypoints. On a larger map, the CPU world may select up to 192 active tiles,
prioritizing the tile under every connected driver and filling surrounding
neighborhoods. A dedicated map worker builds new lanes, road/wheel surfaces,
stop lists, collisions and traffic-light programs **without blocking UDP
keepalives**; the simulation installs the completed region on its next tick.

For a manual test, drive away from the starting area in a compatible client.
Keep another player in the spawn area, then continue across at least several
tile boundaries. Look for `native tile stream:` in the host console and
verify that both players stay connected and their buses continue to update.
Return to the starting area, repeat the journey and reconnect another
client. Note stalls, new invisible walls, missing signals and unexpected
terrain or surface heights. These remain open parity risks until tested on
licensed maps, especially with complex crossings and Chrono content.

This experimental worker is **not** a replacement for scheduled AI or
passenger streaming. AI, pedestrians and timetables remain disabled, and
the server does not authoritatively simulate or execute players' bus scripts.

## Real-client multiplayer test procedure (2–3 drivers)

Use the **same PR #405 client build** on every testing PC, preferably the exact
commit that produced the server package. A stable release may use a different
network protocol or have a different set of player synchronization fields.
The native `headless-test` archive contains **only the server**, not the
graphical clients. Never copy a paid bus or map from one PC to another as a
substitute for a local, licensed installation.

1. Start the server **before** the game clients. Keep its console open. Check
   for `native headless test host ready` and the printed UDP port. If it
   exits during startup, copy the complete console error and stop the test.
2. Start client **A** and join the host by its direct address, or by the
   printed session code if that address is reachable. On a private LAN,
   prefer `<host-local-IP>:27015`. Load the same map installed on the server,
   choose a bus **already present on A**, and move to a known spawn.
3. Start client **B** on a different PC or a separate game process using the
   matching build. Join, load its locally installed bus and park beside A.
   Both clients must independently see **both buses, not a third phantom host
   bus**. Move A, then B; verify each remote bus moves on the other screen.
4. On A change the livery, destination/line and door positions. On B check
   those visible states; repeat B to A. Capture screenshots of the same scene
   if either client disagrees. The server currently **relays** these player
   states; this is not evidence of authoritative server-side vehicle scripts.
5. Close B's session without stopping the server. Verify B disappears for A.
   Rejoin as B or a new client **C** and check that the new join receives the
   surviving bus state. With three simultaneous clients, check each sees
   exactly two remote players. Keep the server's `max_players` above the
   number of test clients, but at or below the documented limit.
6. Run `status`, `list`, `tps` and `timings report` in the host console.
   Record any unexpected disconnects or server errors. Stop the server with
   `stop`, not by terminating its process.

| Scenario | Expected | Result to record |
|---|---|---|
| Join A then B | Both connect to the same world, date and time | Pass / Fail |
| Two moving buses | Each client sees the other's motion, no host bus | Pass / Fail |
| Paint, doors, line, destination | Remote display matches what the sender shows | Pass / Fail, capture mismatch |
| B leaves and C joins | Departed vehicle is removed; slot can be reused | Pass / Fail |
| Three simultaneous clients | Each client sees two remote vehicles | Pass / Fail |
| Security | No map, vehicle, skin or texture transferred by the host | Pass / Fail |

When reporting a failure, include OS, PR commit, map and bus identifiers,
whether connection used an IP or code, the **first error message** and the
matching excerpt of the server and client logs. Redact join codes, passwords,
private addresses and identifying player information before posting publicly.

**Current limits:** AI vehicles, pedestrians, scheduled traffic, server-side
bus/physics authority, distant-tile streaming and complete map/crossing
collisions are incomplete or disabled. They are not acceptance expectations
for this restricted relay test, and passing the table does not make the
native executable safe for production.

## Acceptance checklist

- The process loads the licensed map without graphics or sound initialization.
- Two real game clients connect and see each other's bus positions and movement.
- Server `tps` and `timings report` reflect completed CPU simulation ticks.
- `stop` closes the session; reconnects, bans and malformed join attempts
  do not crash the host.
- Private maps, bus files and liveries are **not** uploaded or redistributed.

**What is not implemented yet:** authoritative bus-script and vehicle-physics
parity, complete map/deformed-road collision, train signals, scheduled/random
AI and passenger snapshots. The unit suite tests two headless LAN clients,
but these in-game acceptance tests must still be done. They block merging the
full replacement server into main.

# openOMSI — native dedicated server test package

**EXPERIMENTAL CPU-ONLY host. Not a replacement for `openomsi --server`.**
The default test launcher uses the restricted free-roam mode. A separate
simulation launcher enables bounded AI, passengers and schedules for private
tests, as documented below. This binary is built from PR #405 and does not initialize the graphical
renderer, DirectX, Vulkan, or audio. It uses the normal openOMSI UDP session
transport with a fixed CPU-side tick and a local server console.

## First test on Windows (including a Windows VPS)

1. Extract this ZIP to a separate directory. Keep your existing server
   installation untouched.
2. Install or locate your own licensed OMSI 2 content and verify
   `Maps/Grundorf/global.cfg` exists. With default settings every client already needs the map
   and matching buses. For private tests you may explicitly enable the
   existing authenticated map and bus-mod transfer service (below).
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

The sample configuration starts with `traffic=0`, `passengers=0`,
`timetable=0`, `accept_uploads=0` and `share_maps=0`.
Public listing, tunnels, verified parked buses and redistribution-authorized
content sharing are independent **explicit opt-ins**, disabled by default
in this example. Full process restart is required to change those network,
fleet and transfer policies. To use another installed map, change only the
`map` entry initially and ensure it contains usable paths.

**Important:** `public_address=off` prevents automatic address advertisement;
it is **not a firewall**. Restrict your server's UDP/TCP ports to trusted
testers using your host/VPS firewall. Do not post the join code publicly.

For a local web admin/TPS panel, set a unique `admin_password` of at least
12 characters in your private `server-test.cfg` and leave
`admin_web=local`. Never publish or commit this password. Open the local
admin URL `http://127.0.0.1:27025/admin/` on the server itself. With a blank
password, web admin is intentionally disabled.

## Optional AI, passenger and timetable simulation (experimental)

The original `--experimental-free-roam` mode remains unchanged, with
`traffic=0`, `passengers=0`, and `timetable=0`. The **separate**
`--experimental-simulation` flag uses the real CPU `TrafficSim`,
`PeopleSim` and `ScheduleSim` from the shared engine:

```sh
cargo run --locked --release -p omsi-server --bin omsi-server -- /path/to/OMSI2 scripts/server-headless/server-simulation.cfg --experimental-simulation
```

The headless-test ZIP now includes `server-simulation.cfg` and
`run-simulation.cmd` (Windows) or `run-simulation.sh` (Linux), alongside
the unchanged free-roam launchers. Run the simulation launcher with your
OMSI 2 installation folder, just as with `run-test`. Alternatively invoke
`omsi-server` directly with the flag and configuration above.

Change the example `map` only to a map actually installed on the test
machine. The native simulation reads its `ailists.cfg`, timetable, road
paths, Chrono data, stop locations and installed AI vehicle scripts.
It runs a bounded number of road vehicles, their existing AI physics and
script callbacks, and pedestrians around connected players. It publishes
cars through the acknowledged LAN traffic table and people through existing
`WORLD` and `DESC` packets. Chrono-based road-bus departures and lines are
experimental; not all consist, train and timetable edge cases are supported.

This test mode permits configuring up to **320 ordinary AI cars**,
**2,000 humans** and **12 scheduled road buses**, without renderer or audio.
The example uses much lower values; these are configurable ceilings, not
guaranteed simultaneous active counts or tested performance targets.
Scheduled buses have a separate budget and are not removed by proximity
recycling. The CPU road network starts with up to **64 entrypoint-oriented
tiles** and supports **additive player-interest tile streaming** while
preserving existing lane indices. Routing across unfinished or untested map
regions may still fail, so test buses and walkers near map edges.

Start with a private LAN/VPS firewall allowing trusted testers. Public
listing and a tunnel are now separate, **explicit operator opt-ins** for
experimental tests; they do not make this a supported production server.
Content sharing stays disabled by default; enabling it intentionally sends
selected files to connected players, so do not use private or paid content
without permission. Neither mode provides authoritative
**player-driven bus** physics/scripts; those remain client-controlled and
validated by the existing network relay. Do not use this experimental mode
for production or publish it as full multiplayer parity.

For a useful manual test, use two matching graphical clients, wait for
AI cars to spawn out of sight, follow them across crossings, approach a
stop with waiting passengers, board a passenger using the existing CLAIM
handshake, then compare traffic, passengers and timetable lines on both
clients. Record CPU/TPS and disconnect/reconnect behavior. Distinguish a
config with no installed AI/human/timetable content from a network failure.


## Public multiplayer testing

To test public discovery through the regular moderated server list, explicitly
enable it in server.cfg and restart the process once:

```ini
public_server = 1
tunnel = 1
public_address = off
share_positions = 0
accept_uploads = 0
share_maps = 0
require_hardware_id = 1
```

The tunnel is optional; with `tunnel=0` you must arrange external reachability
for game UDP and TCP gateway/mod service. Public listing can reject the
server's name, icon or MOTD and requires a reachable gateway. Use moderation,
firewall restrictions and redistribution-authorized assets. Public discovery
is experimental, not proof of complete multiplayer parity.

## Optional private mod-sharing test (server restart required)

The native server reuses the ordinary openOMSI encrypted/authenticated LAN
file service; it does **not** create a new unauthenticated HTTP download
endpoint. For a test where every participant has permission to receive the
content, set these keys explicitly in the active server configuration:

```ini
# Send the map and its non-stock dependencies to joining game clients.
share_maps = 1
# Accept a player's bus, livery and required dependencies for other clients.
accept_uploads = 1
# Optional: verified stored parked buses (up to 32) with uploads enabled.
parked_buses = 0
# Optional: permit downloadable server-owned buses via LISTBUS.
# Only list assets you are explicitly authorized to distribute.
# vehicles = Vehicles/YourBus/YourBus.bus
```

These switches are **independent**: use only `share_maps=1` for map
sharing, only `accept_uploads=1` for client-bus uploads, or both.
With both off (the sample defaults), clients must install their own maps
and vehicles. Missing keys in older configurations do NOT count as
consent: this experimental server refuses to start if the underlying
legacy defaults would enable sharing without an explicit setting.

The file service authenticates to the LAN room, applies trusted-content-root
checks, file type/quarantine limits, hashes and sandboxed downloads. This
does not provide DRM: someone using a modified client can still extract
delivered content. Share only files you own and are allowed to redistribute.
Player-uploaded buses may be retained in the server's LAN store even after
the player leaves, according to `uploads_keep_days`. Test with
public-domain or otherwise redistribution-authorized files first.

**Optional parked buses:** set `accept_uploads=1` and `parked_buses=1..32`
before restarting the server. This reuses the existing verified uploaded-bus
store, claim/hand-over and proximity publication. Parked player buses are
not AI timetable buses and do not gain authoritative player-bus physics.
Keep disabled for content you lack permission to redistribute.

Changes to `share_maps`, `accept_uploads`, `vehicles`,
`uploads_keep_days` or `parked_buses` are restart-only. Running
`/reload` after changing them intentionally rejects the entire reload
without changing the active policy. Stop the server and start it again
using the same `server.cfg`. Never open the mod TCP port to strangers.
The TCP mod service uses the **same numeric port** as LAN UDP (27015
by default); allow TCP 27015 for trusted clients as well as UDP 27015.

## Reload server.cfg without disconnecting players

The running process can re-read the **exact configuration file used at launch**
(`server-test.cfg`, `server-simulation.cfg`, or a custom file). Edit the
file on disk, save it, then enter this command in the **local server terminal**:

```text
/reload
```

The slash is optional (`reload` also works). A successful reload is applied
by the next server simulation tick, without replacing the sockets, game map or
active LAN session. It prints the actual changed setting names to the server
log; an unchanged file prints `no changes`.

The only **hot-reloadable** keys in this experimental native host are:

- `traffic`: random AI population target (up to 320 in CPU simulation).
  Reducing it removes excess unscheduled AI cars; scheduled road buses keep
  their existing trips.
- `passenger_density`: demand factor 0–3, if passengers were enabled at boot.
- `time_speed`: simulation clock speed, 1–30.
- `ai_max_scheduled`: maximum new scheduled road buses (up to 12; 0
  retains the legacy meaning of no configured limit, bounded by the native
  experimental cap of 12).
- `speed_limit_kmh`: 0, 60, 75 or 80.

Changes to **all other** keys (such as `map`, `date`, `time`,
`passengers`, `timetable`, `port`, `web_port`, `max_players`,
`admin_password`, `vehicles`, `accept_uploads`, `share_maps`,
`public_server`, proxy settings and security policy) **require a restart**.
The host rejects the entire reload if it detects any of these changes, even
when the file also includes otherwise hot-reloadable values. It also rejects
missing files, invalid hot values and any attempt to bypass the private-mode
security policy. No changes are applied on a failed reload.

This command is intentionally **terminal-only**, not a Discord/chat command
or remotely invocable web-admin action. Avoid editing a live configuration
while it is half-written; save atomically from your editor if possible. The
initial free-roam mode still requires `traffic=0`,
`passengers=0`, and `timetable=0`; it cannot turn itself into the
simulation mode through `/reload`.

## Restart the CPU world without dropping the tunnel

After editing the **local** configuration, use `/restart` (or `restart`) in
the server terminal to rebuild the CPU map, AI and pedestrian simulation,
timetable and streaming worker. The process does **not** rebind its LAN
socket/session, regenerate the join code, or recreate the existing web
gateway, tunnel or authenticated TCP manifest listener.

Only `passengers`, `timetable` and `ai_max_humans` are additionally
rebuildable with `/restart`. The `/reload` hot values remain accepted
here too. Changes to `map`, ports, public exposure, mod sharing, fleet
authorization, authentication and other restart-only settings still require
**stopping and launching the entire process**. Validation failure leaves
the current world running; it never partially applies a candidate.

The CPU state is rebuilt, so AI and pedestrian entities can be recreated.
Before relying on this command with players connected, test two real game
clients for synchronized removals, AI/timetable state, passenger boarding,
scripts and reconnects. An automated synthetic LAN process regression does
not prove complete in-game continuity. Date-specific Chrono content is
loaded afresh on `/restart` using the current simulation date, but automatic
Chrono map migration exactly at midnight is still a blocking limitation.

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

This dynamic tile worker belongs to **free-roam only**. It still disables
AI cars, pedestrians and timetables. The separate **CPU simulation** mode
provides limited scheduled/random AI and pedestrians, but deliberately
keeps its initial tiles fixed while entities are active. Neither mode
authoritatively executes the **player's** bus scripts or vehicle physics.

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
| Security | Default mode transfers nothing; explicit opt-ins transfer only authorized content | Pass / Fail |

When reporting a failure, include OS, PR commit, map and bus identifiers,
whether connection used an IP or code, the **first error message** and the
matching excerpt of the server and client logs. Redact join codes, passwords,
private addresses and identifying player information before posting publicly.

**Current limits:** the free-roam relay still disables AI traffic,
pedestrians and schedules, but can stream distant tiles. The separate
`--experimental-simulation` mode activates bounded AI traffic, passengers,
Chrono road buses and their shared LAN script-context pages on fixed tiles.
Neither has complete server-owned player-bus physics, map/crossing collision,
entity-aware region streaming or verified real-client passenger parity.
Passing this table does **not** make either native mode production-ready.

## Acceptance checklist

- The process loads the licensed map without graphics or sound initialization.
- Two real game clients connect and see each other's bus positions and movement.
- Server `tps` and `timings report` reflect completed CPU simulation ticks.
- `stop` closes the session; reconnects, bans and malformed join attempts
  do not crash the host.
- Default configuration transfers no private map, vehicle or livery. With explicit mod-sharing opt-in, only redistribution-authorized content is served over the authenticated connection.

**Still missing:** authoritative player-bus physics/scripts, full deformed-road
and train-signal parity, moving active AI/people between streamed tiles, and
authenticated remapping/forwarding of other clients' `WORLDUP`/`DESCUP`
passengers. The native host accepts validated, bounded client-owned upstream queues and forwards
them with isolated IDs to nearby peers; server-owned people and AI also do
publish their own snapshots. Multi-client graphical verification remains a
blocking acceptance gate before merging the replacement server into main.

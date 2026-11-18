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
   `timings on`.

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

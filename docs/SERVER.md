# Dedicated server

## How it runs today

```
openomsi --root /path/to/OMSI2 --server /opt/openomsi-server/server.cfg
```

(`scripts/build-server.sh` builds it on a Linux machine into `dist/server`, and `start.sh`
there starts it; every release also has ready `openOMSI-<version>-server-linux-x64.zip` and
`-server-linux-arm64.zip`, and for Windows (10/11 and Windows Server 2016 or later)
`-server-windows-x64.zip` and `-server-windows-arm64.zip`, started with
`start.cmd C:\path\to\OMSI2`.) The server is the game binary hosting a session with no window, no sound and no
graphics card: the renderer runs on wgpu's no-op device, so the world, the AI traffic, the
timetable buses and the people are simulated exactly as a hosting player's game does, and
nothing is drawn. `server.cfg` is written with commented defaults on the first start (name,
motd, map, date, time, weather, traffic, timetable, passengers, port, web_port,
max_players, tunnel, radius, the voice chat); `server-icon.png` beside it (64x64, like Minecraft's) is the
icon the players' list shows.

**The start script that asks** (`run-server.cmd` on Windows, a double click; `run-server.sh`
on Linux, both beside `openomsi` in the server's zip): the first time it asks for the OMSI 2
folder (and checks it is a complete installation before going on), then for the main
settings of `server.cfg` - name, players at most, the UDP and web ports, the tunnel, the admin
password (typed hidden; 8 characters at least) - with what the file has in brackets, kept
with Enter. Every time, it lists the installed maps by number (the last one marked, taken
with Enter) and starts the server on the one picked, without changing `map` in `server.cfg`;
the output goes to the window and to `server.log`, and when the server stops on an error it
says what the exit code means. Asked once per map, it makes `start-<map>.cmd` (or `.sh`)
beside it: that starts the same map again with no question. `run-server.cmd -Setup`
(`run-server.sh --setup`) asks for the folder and the settings again; `-Map maps/<folder>/global.cfg`
(`--map …`) starts a map with no question. What it remembers is in `run-server.conf` (the
folder, the last map, the maps of the `start-` scripts). It speaks Portuguese on a system in
Portuguese, English otherwise. It needs no rights of an administrator; on Windows it runs in
the Windows PowerShell every Windows has. `start.cmd` / `start.sh` stay as they were, for a
service or another launcher.

For a launcher of its own, the program has the two pieces the script uses:
`openomsi --root <folder> --list-maps` prints the installed maps (one a line, as `server.cfg`
names them; the content folder's too) and ends - with code 1 and what is missing when the
folder is not a complete OMSI 2, never another installation found instead; with
`--server <server.cfg>` it also writes a missing `server.cfg` with its defaults, nothing more
of a server's start. `--server-map maps/<folder>/global.cfg` starts the server on that map
instead of `server.cfg`'s.

**When it does not start**, it says why on its output and ends with a code a launcher can
tell apart:

| exit code | cause |
|---|---|
| 1 | the original OMSI 2 was not found at `--root` (the files missing are listed: a wrong folder, or an incomplete installation), or another error, printed as `Error: …` |
| 2 | `server.cfg` cannot be read or written, or its `map` is not installed (the maps there are listed, as `server.cfg` names them) |
| 3 | its UDP `port` is taken (another server, or another program): stop it, or choose another port |

A `web_port` that is taken is no reason to stop: the gateway takes another port and the log
says which (the status page, the WebSocket players and the tunnel are there). The log goes to
the standard error output: a launcher keeps it with the rest (`start.sh` writes it to
`server.log` too; `start.cmd` shows it in its window), and does not take a line there for a
failure - informational lines go there as well; the exit code says whether it failed. Paths
with spaces or accents are passed as they are, quoted. A map is named as in the game, relative
to the OMSI 2 folder (`maps/<folder>/global.cfg`), and found in the content folder (mods) too.

Players reach it two ways:

* UDP at `host:port` (a server with a public address, or on the LAN);
* over a **WebSocket** at its web port (`omsi-net::ws`): every datagram is one binary
  message, the server's gateway gives each WebSocket a UDP socket of its own on 127.0.0.1.
  The session sees such a player there (`joined from 127.0.0.1:<port>`); the log says who
  that is (`gateway: player WebSocket from <address> on 127.0.0.1:<port>`), with the
  address a reverse proxy or tunnel on the machine forwards (`X-Forwarded-For`,
  `X-Real-IP`, `CF-Connecting-IP`).
  That is what a free Cloudflare quick tunnel carries (`tunnel = 1` starts `cloudflared
  tunnel --url http://127.0.0.1:<web_port>` and prints the `https://….trycloudflare.com`
  address). The same port answers `GET /status` (JSON: name, motd, map, players,
  max_players, time, weather, version, protocol, vehicles, and on a dedicated server `world`:
  its AI cars, buses, cars asleep, parked cars, people walking, waiting and aboard, the
  traffic density) and `GET /icon.png`, which the launcher's Multiplayer → Servers list
  shows. `vehicles` is the server's `vehicles` list (else every bus it has): a joining
  player's launcher offers only those, and so do the game menu's *Place a vehicle* and *Swap*
  once the game has joined. With an `admin_password` it also takes `POST /admin`
  from the machine itself: one admin command a line (`clock 30600`, `weather set
  Weather/#CAVOK.owt`, `say …`, `kick 3`, as the Administration menu sends them), the
  password in `X-Admin-Password`; five wrong ones in two minutes close it for a while. A
  tool beside the server (a dispatch page, a script) administers it that way without
  joining. A reverse proxy on the same machine connects from 127.0.0.1 as well: do not let
  it pass `/admin` on. With `share_positions = 1` it also answers
  `GET /players`: a JSON array of the players (`id`, `name`, `bus`, `line`, `destination`,
  `tour`, `x`/`y` in world metres east/north, `heading` in degrees clockwise from north,
  `speed_kmh`, `on_foot` and `aboard` - a player on foot is where it walks (even with its
  own bus parked nearby), one sitting in another player's bus is with that bus (its id) - and `lat`/`lon` on a `[worldcoordinates]`
  map such as Berlin-Spandau, `null` elsewhere), refreshed every second - what a live map of
  the server on a website needs.
  It is off by default: the players' names and positions are then nobody's business.

With `voice_channel` set, the players **talk** to each other through
[GreenTeaSpeak](https://greenteaspeak.de) as SaltyChat lets FiveM players do: a player is
heard from where they stand or sit, up to `voice_range` metres (20 by default), and through
the bodywork - quieter - when one of the two is in a bus and the other is not. Each player
runs GreenTeaSpeak 2 with the openOMSI plugin (`tools/greenteaspeak-plugin`, its README says
how to install it) and is connected to the voice server whose unique id is
`voice_server_uid` (the id in GreenTeaSpeak's server info panel; it is needed: without it
there is no voice chat, so that a server cannot move its players about on whatever voice
server they happen to be on). A
joining game asks the server for these settings (the command `voice?`), the plugin moves
the player into the channel `voice_channel` (its id or its name, with
`voice_channel_password`) and renames them `<name> #<player id>` - the name every other
game of the session gives them there - and the game tells it ten times a second where the
camera is and where everybody else is. A player who is speaking has "speaking" under their
name tag. With `voice_radio` on (the default), a driver is put on a map-wide **bus radio**
automatically: hold the bindable *Multiplayer: bus radio* key (Controls; X by default) to
transmit to every other driver on the map while proximity voice still reaches people next
to the bus; their name tag shows "radio". A player hosting by code names the voice server in
`~/.openomsi/voice.cfg` with the same keys. Settings → General → *Voice chat through
GreenTeaSpeak* switches it off. `voice_channel_password` is sent to every player who joins
(their game needs it to enter the channel): it keeps strangers on the voice server out of
the channel, not the server's own players. The settings together must fit a chat command
(160 characters after encoding), or the server says so in its log and has no voice chat.

The same gateway and tunnel open for a player hosting by **code** (Connect by Code): its
address goes to the rendezvous topic, and a joining game that gets no answer from the
addresses in the code (routers that cannot be punched through) joins through the tunnel.

What is still to come from the plan below: a world without the renderer's data structures
(the server keeps the tiles' meshes in memory for nothing), tiles streamed by the players'
interest (today `radius = 0` loads the whole map), a password, admin commands.

# Dedicated server - the original concept

Today a session is hosted by a player's game: the host simulates the shared world (AI
traffic, timetable buses, people, traffic lights) around every player and streams it to the
clients (`omsi-net::world`, `omsi-app::lan_world`). A dedicated server is the same host role
without a player, a window or a sound card, running on a machine that is always on - like a
Minecraft server. This document fixes how it is to be built, so that the pieces written now
fit it.

## What already fits

* **The protocol.** Clients do not care whether the host is a game or a server: `HELLO` /
  `WELCOME` / states / world frames / claims are the same (`omsi-net`, `PROTOCOL`).
* **Authority.** The host already owns the shared world; a client simulates only its own
  bus and the people boarding it, and claims waiting passengers from the host
  (`lan_world`). A server keeps exactly that split.
* **Finding it.** A session code carries the session id; the rendezvous (`omsi-net::bridge`:
  STUN, UPnP, the relay topic named after the id) finds the host behind a home router. A
  server with a public address is simply reachable at `address:port`; it posts to the relay
  as a host does, so a code works for it too.
* **Areas of interest.** Traffic and people are already populated around *every* player
  (`Traffic::lan_centers`, `Humans::lan_centers`) and sent per client only near its bus.

## What has to move

1. **A world without a renderer.** `scene::World` loads tiles for drawing and for the
   simulation at once, and `Traffic`/`Humans` create render instances as they spawn. The
   simulation part goes into its own crate (`omsi-world`: tiles' lanes, stops, objects'
   collision and light programs, the timetable, `Traffic`, `Humans`) behind a small trait
   the game implements to show things:

   ```rust
   pub trait Presenter {
       fn vehicle_added(&mut self, id: u32, ty: &VehicleType, scheme: Option<usize>);
       fn vehicle_removed(&mut self, id: u32);
       fn person_added(&mut self, id: u32, hum: &Path);
       fn person_removed(&mut self, id: u32);
       // poses are read by the presenter from the world after each tick
   }
   ```

   The game's presenter makes render instances (today's code in `Traffic::sync`,
   `Humans::tick`); the server's does nothing. Vehicle scripts run without their meshes
   (`VehicleInstance` already works headless: the `bus_audit` example does it).
2. **Tiles by interest, not by camera.** The tile streamer follows the camera; the server
   streams the tiles around every player's bus (the union of their radii) and lets go of
   the rest.
3. **A player without a bus of its own on the server.** The server is player 0 with no pose;
   `LanSession` already tolerates a host whose pose has no vehicle (`Pose::has_vehicle`).

## The server program

`omsi-server` (a binary of its own, no winit/wgpu/cpal):

```
omsi-server --root "/path/to/OMSI 2" --config server.cfg
```

`server.cfg` (key = value, like `settings.cfg`):

| key | meaning |
|---|---|
| `map` | `maps/Berlin-Spandau/global.cfg` |
| `date`, `time`, `time_scale` | the world clock (the server's clock is the session's) |
| `weather` | a `.owt`, or `auto` (a cycle) |
| `traffic`, `passengers`, `timetable` | density and switches, as the launcher's |
| `port` | UDP port (27015) |
| `max_players`, `password` | who may join (`REJECT` with a reason otherwise) |
| `name`, `motd` | shown in the launcher's list and on joining |
| `public` | post to the relay's lobby topic, so the launcher can list the server |
| `admins` | player names allowed to run `/kick`, `/time`, `/weather` in the chat |

It needs the original OMSI 2 files like the game (same `missing_original_essentials`
check) and the same mods as the players: the server lists its content in `WELCOME`
(vehicle and map files with sizes), and a client missing something is told what.

## Order of work

1. Extract `omsi-world` with the `Presenter` trait; the game keeps working through it.
2. Headless world tick in a test (`Traffic` and `Humans` on Grundorf for 10 minutes without a
   renderer, compared with the game's positions for the same seed).
3. `omsi-server` hosting a session from `server.cfg`; the launcher's Join accepts
   `host:port` and codes as today.
4. Lobby: servers post `name/map/players/code` to the relay's lobby topic; the launcher
   shows the list.
5. Admin commands and a small status page (players, tick time, bandwidth).

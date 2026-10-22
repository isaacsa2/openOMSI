# Command reference — game and dedicated server

This public guide covers supported, non-destructive startup commands,
not internal testing, fault-injection or protocol switches. The source
of truth for your installed build is **openomsi --help**. A complete
original OMSI 2 installation is required. Quote paths with spaces.

## Normal game

~~~powershell
.\openomsi.exe --root "D:\Games\OMSI 2"
.\openomsi.exe --root "D:\Games\OMSI 2" --no-menu --map "maps/Grundorf/global.cfg" --bus "Vehicles/MAN_SD200/MAN_SD80.bus"
~~~

Without startup arguments openOMSI normally opens the launcher.
Use --menu for the in-game start menu or --no-menu for direct launch.

| CLI flag | Purpose |
| --- | --- |
| --root PATH | OMSI 2 installation. |
| --map maps/NAME/global.cfg | Choose the map. |
| --bus Vehicles/.../BUS.bus | Spawn the player's bus. |
| --entry N / --auto-entry | Map entry index or duty-based automatic entry. |
| --spawn x,y,heading[,z] | Explicit spawn coordinates. |
| --view driver / pax / outside / free | Starting view. |
| --time HH:MM / --date YYYY-MM-DD | Starting time and date. |
| --season winter / summer / spring / autumn | Override season; supports -early, -mid, -late. |
| --weather Weather/FILE.owt | Select weather; natural/cycle also supported. |
| --traffic N | Number of AI traffic vehicles requested. |
| --schedule / --no-timetable-buses | Run timetable / omit AI timetable buses. |
| --line ID --tour ID / --trip ID | Choose a scheduled duty or trip. |
| --passengers / --riders N | Enable passengers / start with riders on board. |
| --paint NAME / --hof NAME | Bus paint and depot HOF. |
| --plate TEXT / --number TEXT | Registration and fleet number. |
| --autostart / --on-foot | Put the bus in service / start on foot. |
| --drive-keys simple / wasd / arrows / omsi | Driving controls. |
| --view-distance METRES | Map streaming view distance. |
| --situation FILE.osn | Restore a saved situation. |
| --refuel / --wash / --repair | Depot service actions. |
| --dirt 0..1 | Initial vehicle dirt level. |
| --content-zip FILE.zip | Add a content archive; repeatable. |
| --enhanced / --enhanced-plus | Enhanced renderer or Enhanced+ ray tracing when supported. |
| --lan-host PORT / --lan-join HOST[:PORT] | Host or join multiplayer. |
| --lan-name TEXT | Display name in LAN play. |
| --export-diagnostics FILE.zip | Write support diagnostics and exit. |

Other useful public flags: --driver, --depot, --keep-time,
--whole-tour, --tutorial N and --physics rigid/simple.
See USER_GUIDE.md for controls and game menus.

## Automation and performance tests

| CLI flag | Purpose |
| --- | --- |
| --no-menu | Directly run the supplied configuration. |
| --offscreen FILE.png --size 1600x900 | Render a still image without a game window. |
| --cam x,y,z,yaw,pitch | Camera for offscreen image. |
| --radius N / --all | Load a limited tile radius or every tile (memory intensive). |
| --exit-after SECONDS | End a **windowed** test automatically, enabling JSON profiling. |
| --save-situation FILE.osn | Write a saved state after the drive. |

For non-blocking benchmark configuration see PROFILING.md.
Specialized internal test commands are deliberately excluded.

## Dedicated server

The server runs without a game window and uses the same executable.

~~~powershell
.\openomsi.exe --root "D:\Games\OMSI 2" --server ".\server.cfg"
.\openomsi.exe --root "D:\Games\OMSI 2" --server ".\server.cfg" --server-map "maps/Grundorf/global.cfg"
.\openomsi.exe --root "D:\Games\OMSI 2" --list-maps
~~~

| CLI flag | Purpose |
| --- | --- |
| --server FILE.cfg | Run headless server; write commented defaults if missing. |
| --server-map maps/NAME/global.cfg | One-run override for the map in server.cfg. |
| --list-maps | Print installed maps and exit. |
| --root PATH | Original OMSI 2 content, also required by the server. |

Release server packages also include start.cmd/start.sh and interactive
run-server.cmd/run-server.sh helpers.

### Safe starter server.cfg

~~~ini
name = Local test server
motd = Welcome!
map = maps/Grundorf/global.cfg
date =
time = 08:00
weather =
traffic = 20
timetable = 1
passengers = 1
passenger_density = 1
port = 27015
web_port = 27025
max_players = 8
tunnel = 0
radius = 0
admin_web = local
public_server = 0
share_positions = 0
share_maps = 0
~~~

passengers is **on/off**, NOT a passenger count. passenger_density
is a multiplier of the map's demand (0–3); radius=0 loads the
**whole map** and may need significant RAM.

### Configuration-key reference

| Keys | Purpose |
| --- | --- |
| name, motd | Public server name/message. |
| map, date, time, weather | Map and initial world state. |
| time_speed, real_time | Simulation clock speed or host clock. |
| metar_sync, metar_station | ICAO weather station syncing. |
| traffic, timetable, passengers, passenger_density | AI traffic and people. |
| port, web_port, max_players | UDP game port, TCP gateway port, capacity. |
| tunnel, public_server | Optional tunnel and public-server listing. |
| radius, vehicles | Map tile radius and vehicle allowlist. |
| accept_uploads, uploads_keep_days | Client vehicle uploads and retention. |
| share_maps | Distribute server map content (large transfer). |
| share_positions | Publish player positions; off by default. |
| voice, voice_radio, voice_range | Voice-chat policy. |
| admin_web | Prefer local or off. |

Credentials, remote web administration, trusted proxies, ban controls
and privileged administration commands are intentionally *not* in
the safe starter. SERVER.md describes their security implications.
Never publish passwords, private URLs or tokens in bug reports.

For environment variables see ENVIRONMENT_REFERENCE.md.

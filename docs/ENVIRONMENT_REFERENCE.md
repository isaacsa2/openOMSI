# Public environment-variable reference

This is a **curated** guide for normal game and dedicated-server
operation. It intentionally omits fault-injection hooks, internal
protocol overrides, resource-limit experiments, private keys and
intrusive debug traces. For exact definitions the source of truth
is crates/omsi-cfg/src/flags.rs.

**Important:** registered boolean OMSI_* flags are enabled by being
*present*, even when their value is "0" or empty. Unset the variable to
disable it. Most values are read once and cached; restart the process
after changing them. Prefer ordinary launcher settings or server.cfg.

## Game/launcher settings

| Variable | Example / effect |
| --- | --- |
| OMSI_ROOT | D:\Games\OMSI 2 — installation root. |
| OMSI_CONTENT | Optional separate mods/content directory. |
| OMSI_CONTENT_ZIP | Extra archives separated by the OS PATH separator. |
| OMSI_BACKEND | vulkan / dx12 / gl / angle / metal (where supported). |
| OMSI_GRAPHICS | vanilla / vanilla_plus / enhanced. |
| OMSI_ENHANCED | Presence requests Enhanced graphics. |
| OMSI_ENHANCED_PLUS | Presence requests Enhanced+ (ray tracing if supported). |
| OMSI_MAX_FPS | 60 — frame rate limit. |
| OMSI_PAX_ANIMATION | original / enhanced — passenger animation. |
| OMSI_CHARACTER_MODELS | original / improved — character models. |
| OMSI_WINDY_TREES | 0 / 1 — wind animation (text-valued, not a presence flag). |
| OMSI_OPENXR | Presence enables supported Windows VR/OpenXR mode. |

An API override can affect performance and stability; remove it to
restore normal selection.

## Performance diagnostics

| Variable | Effect |
| --- | --- |
| OMSI_PROFILE | Presence enables CPU stage/frame timing. |
| OMSI_GPU_TIMERS | Presence enables asynchronous GPU pass timestamp queries where supported. |
| OMSI_PROFILE_JSON | Path to JSON summary with OMSI_PROFILE and --exit-after. |
| OMSI_DEBUG_DRAWS | Logs draw-instance changes (verbose). |
| RUST_LOG | Standard runtime log filter such as info or warn. |

See PROFILING.md for an example that avoids an explicit GPU wait.
Do not enable undocumented GPU-synchronization or driver-override
test switches for baseline measurements.

## Dedicated server

The dedicated server normally uses server.cfg for the world,
multiplayer, visibility and security policy. These environment
variables are useful for process startup:

| Variable | Effect |
| --- | --- |
| OMSI_ROOT | Original OMSI 2 content path. |
| OMSI_CONTENT | Additional content/mod root. |
| OMSI_CONTENT_ZIP | Extra content archives. |
| OMSI_CLOUDFLARED | Location of the cloudflared executable when a tunnel is explicitly configured. |
| RUST_LOG | Log verbosity, e.g. info or warn. |

Rendering-specific flags do not tune a headless dedicated server.
See COMMAND_REFERENCE.md and SERVER.md.

## Windows PowerShell example

~~~powershell
$env:OMSI_PROFILE = "1"
$env:OMSI_GPU_TIMERS = "1"
$env:OMSI_PROFILE_JSON = "$PWD\profile.json"
# Run a windowed openomsi.exe session with --exit-after here.
Remove-Item Env:OMSI_PROFILE -ErrorAction SilentlyContinue
Remove-Item Env:OMSI_GPU_TIMERS -ErrorAction SilentlyContinue
Remove-Item Env:OMSI_PROFILE_JSON -ErrorAction SilentlyContinue
~~~

## Linux/macOS shell example

~~~sh
OMSI_PROFILE=1 OMSI_GPU_TIMERS=1 OMSI_PROFILE_JSON="$PWD/profile.json" \
  ./openomsi --root "/path/to/OMSI 2" --no-menu --exit-after 120
~~~

Adjust map/bus arguments when needed. Never publish environment dumps
containing API keys, passwords, local network identifiers, tokens or
sensitive paths.

The generated docs/DEBUG_FLAGS.md documents *internal* options too:
it is **not** a recommended list of safe switches.

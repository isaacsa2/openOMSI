# Native launcher fallback

A bare start of `openomsi` still prefers the separate Tauri launcher. If that program is
missing, cannot be spawned, or exits with an error during its existing two-second startup
probe, the game opens its existing keyboard-driven native start menu. It uses the game's
renderer and HUD; it does not link WebView2, WebKitGTK or Tauri into the game. This reuses
`--menu`, the content readers, launcher-core's saved-situation discovery and the ordinary
session loader instead of restoring the former full wgpu launcher or adding a second UI
system. An explicitly configured `OMSI_LAUNCHER` override retains its existing behavior.

## Scope

The menu offers map, vehicle, hour, date/season (day of year in the default year), weather,
traffic, passengers, timetable on/off and saved situations. Up/down selects a row; left/right
changes it; Enter starts and Escape exits. Saved situations include the original
`Situations/*.osn`, per-map save slots, last situation and quicksave, including the writable
fallback content folder, through launcher-core. A saved situation supplies its own map,
vehicle, date and time as in the regular loader.

There are no advanced settings, mod manager, profile editor, updater, 3D bus previews or
multiplayer administration pages. Installed content and the player's existing settings and
plugin choices are retained. Timetable selection here is a switch, not Tauri's duty picker.
The existing native menu's labels and keyboard navigation are retained.

## Recovery and limits

- Windows keeps the existing missing-WebView2 installer offer and installed-runtime
  profile-repair retry. Declining the installer or an unsuccessful repair allows fallback.
  A registered runtime whose files are missing follows the existing repair diagnostic and
  also allows fallback. The startup watchdog pauses its deadline while the player answers
  the installer prompt or the installer runs; it still detects a dead process. Repair does
  not mark the launcher healthy, and startup gets a fresh deadline afterwards.
- The native menu marks the guarded start healthy only after presenting a menu frame, not
  merely on a redraw request. Once presented, the player can leave it open indefinitely.
  A genuine failure before presentation still uses the existing startup recovery policy.
- Without the original OMSI 2 installation, fallback shows the existing missing-content
  message (dialog on Windows and, when available, Linux; stderr/log everywhere), listing
  missing files and the `--root` command. It ends cleanly rather than asking the guard to
  update/roll back a working binary or enter safe mode for missing content.
- A Tauri process alive after the two-second probe is accepted. A blank or unresponsive web
  page, or a failure after the probe, is **not detected** by this mechanism. The parent game
  has already returned, so its watchdog cannot monitor that page. A clean early launcher
  exit is also respected as an intentional close. Close a stuck Tauri window and use
  `openomsi --menu` (with `--root` if necessary). There is no new page heartbeat or polling.
- The fallback still needs the game's graphics backend, native libraries and valid content.
  WebView independence does not make unsupported GPUs work. Windows/Linux GUI and actual
  content tests must be performed on their respective systems before this draft is ready.

## Performance

No dependency or persistent WebView probe is added. The normal Tauri and fully specified
gameplay paths keep their existing session logic. The fallback uses existing game renderer
initialization, so it is not a CPU-only dialog and its pipeline/memory costs still apply.
The native menu uses its existing redraw loop; WebView detection does not add GPU work.
Save discovery happens once when opening the start menu, not on redraw or in in-game
vehicle pickers. The healthy notification is internally once-only. No FPS or memory
improvement is claimed without platform measurements.

## Manual test matrix

Use a disposable extracted test build and a test account/VM for dependency removal and
startup-recovery tests. Keep the original package files to restore afterwards. Do not
remove system libraries from a normal workstation. Record the build SHA, OS, GPU/backend,
launcher/game/guard logs and screenshots for each case.

| Case | Procedure | Expected result |
|---|---|---|
| Windows, WebView2 installed | Start `openomsi.exe` with no arguments. | Normal Tauri launcher; starting a duty works as before. |
| Windows, WebView2 absent | In a VM without the runtime, start normally; decline the install offer. Repeat with an unsuccessful installer and with a successful install. | Decline/failure opens native menu; successful install retries Tauri. |
| Installer vs watchdog | Leave the offer open longer than the default 120-second deadline, then decline. Also test an installer taking longer than that. | No watchdog rescue during deliberate repair; native menu can then present and remain open. |
| Linux, WebKitGTK 4.1 absent | Run the packaged game in a graphical VM without `libwebkit2gtk-4.1`, with its ordinary graphics/native dependencies installed. | Tauri's immediate loader error leads to native menu. |
| Launcher missing | Rename only the packaged `openomsi-launcher-app[.exe]`, then start the game bare. | Native menu opens; restore the original launcher afterwards. |
| Launcher fails immediately | In a test package, replace the launcher with a small test executable returning a nonzero exit code. | Native fallback after the existing platform repair attempt, if applicable. |
| Menu idle and exit | Leave the native menu open for over 120 seconds, navigate all nine rows, then press Escape; repeat using Enter. | No watchdog recovery while choosing; Escape exits, Enter loads the chosen session. |
| Missing content | Use a test account with no discoverable OMSI 2 installation and a missing launcher. | Actionable missing-content error; clean exit and no release-repair attempt. |
| Explicit native menu | Run `openomsi --menu --root "/path/to/OMSI 2"` (`.exe` on Windows). | Same menu without starting Tauri or offering a WebView install. |
| Saved situation | Save a slot and a quicksave, including a save in the writable fallback folder; reopen native menu and select each. | Existing loaders restore the chosen save's map, bus, date and time. |
| Blank/unresponsive Tauri | Keep a launcher process alive with an unresponsive/blank page past two seconds. | Automatic fallback is not expected; close it and start explicitly with `--menu`. |

For bare launches with content outside the default locations, set `OMSI_ROOT` to the
installation path, or first run `openomsi --menu --root "/path/to/OMSI 2"` to remember it.
Test the normal launcher duty, saved-situation and tutorial paths too, with the same settings
and plugin choices. Compare screenshots of normal Tauri startup and of the native menu;
attach them to the PR rather than committing images or original game content.

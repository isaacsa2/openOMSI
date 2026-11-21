# Proton Bus → native openOMSI formats (PR #220)

Updated 21 November 2026. This is the revived experimental local Proton Bus converter, built on the current openOMSI fork main. The complete 13 October version is preserved in `archive/pr220-legacy-20261013`.

## Design

Conversion now **writes the game's ordinary supported formats directly**. There is no second encoding/renaming pass to the experimental `OPENOMSI-OI` envelope for newly generated content. This removes extra compression/rewrite work and lets the current map, scenery, mesh and vehicle parsers read the generated assets.

| Output | New filenames / representation |
| --- | --- |
| Map manifest | `global.cfg` |
| Tiles / terrain | `tile_X_Y.map` and `tile_X_Y.map.terrain` |
| Scenery / collision | `proton-import/*.sco` |
| Mesh geometry | `*.o3d` |
| Depot and name hint | `proton-import/depot.hof`, `proton-import/depot.txt`, shared `HOFs/*.hof` |
| Converted rigid vehicle | `Vehicles/Proton-*/vehicle.bus` |
| Vehicle models, scripts and sounds | `Model/*.cfg`, `Script/*.cfg`, `Sound/*.cfg`, `Model/*.o3d` |
| Static AI vehicles | `proton-import/ai/*/vehicle.ovh` and native model files |
| Supporting navigation, traffic and itineraries | `proton-import/*.json` |

**These are openOMSI-readable outputs, not a promise that every extension or gameplay feature works in the original OMSI 2 executable.** Some generated metadata and extensions remain specific to openOMSI.

The `omsi-cfg::oi` codec and readers are retained to avoid breaking already-imported experimental `.oi` content. Navigation, AI metadata, depot hint and other relevant readers use native paths by default and explicit legacy fallbacks where provided. New native files must **not** be given an OI header.

The converter validates generated native `global.cfg` and tiles with `omsi-map` before completing the conversion. References to generated scenery must stay inside the imported map and point to existing native `.sco` files. Existing texture/mesh-size budgets and original format validation remain in place.

## Integration

- Recovered `omsi-proton` converter and original tests and examples.
- Mods installs selected Proton maps using explicit opt-in, bounded source inspection, staging, cancellation, disk-space checks and normal mod ownership.
- Map listings, vehicles, depots, AI metadata and imported navigation have been adapted to the current openOMSI parsers.
- Map demand without a timetable and some traffic/vehicle integration are implemented, subject to in-game validation.
- Creator-supplied assets are converted **locally**; none are bundled with openOMSI. Source files, credits and licence bytes remain preserved.

## Confirmed validation

On GitHub Actions for the native-output branch:

- `cargo check -p omsi-proton -p omsi-map -p omsi-launcher-core -p omsi-scene --all-targets`: **passed**.
- `cargo test -p omsi-proton --lib`: **109 passed, 0 failed**.
- `cargo test -p omsi-launcher-core install::proton::tests --lib`: **9 passed, 0 failed**.
- `scripts/size-budget.sh`: **passed (1,364 files checked)**.

The original recovered OI codec tests passed in an earlier phase. The focused migration workflow itself completed successfully. This **does not prove full gameplay support**.

## Status after the November 21 main sync

- The feature branch is synchronized with fork `main` and has no commits behind it.
- Linux `cargo check --locked -p omsi-proton -p omsi-launcher-core -p omsi-scene -p omsi-sim --all-targets` passed on the merged tree. This is a compilation check, not a runtime conversion test.
- The unscheduled-passenger fixture no longer assumes a bus is boardable without a live `BusNow` and valid entry side.
- Windows converter binaries, full CI and real-content in-game validation are still required before merge.

## Remaining before merge

- [ ] Confirm the complete game builds and tests on all target platforms.
- [x] Resolve rustfmt complaints in the previously touched converter/scene/world files and regenerate the launcher lockfile.
- [ ] Verify rendering, lighting, collision, vehicle physics, animation, native traffic, passengers and navigation in-game using legitimately provided test assets.
- [ ] Run installation/uninstallation and corrupt-input regression checks across all supported archive/folder modes.
- [ ] Finish UI translation, user instructions and platform release packaging when stable.

## Using the converter locally (preview)

The converter never bundles someone else's Proton Bus assets. Obtain a local source mod that you own or are authorized to convert. Its original main `.map.txt`, `.bus.txt` or `.ini` configuration must contain a single author-provided declaration:

```ini
[openomsi]
allowConversion=1
```

Install a current Rust toolchain and run these commands from the repository root. Use a **new destination folder outside the source folder** for each conversion.

```powershell
# Proton Bus map (unpacked source folder containing exactly one *.map.txt)
cargo run --release --locked -p omsi-proton --example import_map -- "C:\\ProtonSource\\City" "C:\\ProtonOutput\\City" "City"

# Inspect a source vehicle without modifying it
cargo run --release --locked -p omsi-proton --example inspect_vehicle -- "C:\\ProtonSource\\Bus\\vehicle.bus.txt"

# Export a rigid vehicle to a new openOMSI Mods package
cargo run --release --locked -p omsi-proton --example import_vehicle -- "C:\\ProtonSource\\Bus\\vehicle.bus.txt" "C:\\ProtonOutput\\Bus"
```

If you have the dedicated Windows CLI test builds, replace `cargo run --release --locked -p omsi-proton --example <name> --` with the matching `import_map.exe`, `inspect_vehicle.exe` or `import_vehicle.exe`. Map packages can also be selected in the Mods installer and must be installed with **Extract**, not InPlace. A generated package is intended for the openOMSI format readers; do not assume that every Proton vehicle subsystem, script or original OMSI 2 feature will work before inspecting conversion warnings and validating the result in-game.

## Author rights

The exact declaration `[openomsi]\nallowConversion=1` remains mandatory for each eligible conversion entrypoint. It is **editable text, not cryptographic proof of ownership or permission to redistribute**. Respect attribution and licences; never bundle or share someone else's map, bus, model, script, sound or texture without appropriate permission.

**PR #220 remains Draft; no merge to main or upstream.**

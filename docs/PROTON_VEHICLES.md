# Proton vehicles: preparation only

Vehicle support is **not a drivable-bus converter yet**. The map converter, its required
`[openomsi] allowConversion=1` declaration and generated OI package remain unchanged.
No vehicle conversion entry point or permission bypass is introduced.

A read-only inspector in `omsi-proton::vehicle` recognizes a modern `.bus.txt` main file
or legacy `.ini`, reads version 1–2, resolves the authored `baseDir` safely and lists
models, automatic/manual engine definitions, wheels, lights, sounds, seats, doors,
texture and skin folders. It reports mass, template and the driver's camera, and
which doors admit or discharge passengers. INI positions use the existing Unity
Y-up to engine Z-up mapping. Missing references are reported; traversal and symlinks
are rejected. Configuration reads are bounded, and no meshes are retained or loaded
into a renderer. The inspector writes no output package and does not alter the source.

```powershell
cargo build --release -p omsi-proton --example inspect_vehicle
.\target\release\examples\inspect_vehicle.exe "C:\Bus\Example.ini"
```

On Linux/macOS:

```sh
cargo run -p omsi-proton --example inspect_vehicle -- /bus/Example.bus.txt
```

Use an unpacked complete source package. The JSON written to stdout is an inventory,
not a vehicle that the game can load. Credits and licenses stay in the original package.
Inspection or public availability does not establish redistribution rights.

## Next implementation steps

- Reuse the existing streaming 3DS reader, shared material/texture conversion and OI
  writer; do not fork or duplicate the map/AI implementation.
- Generate a native driveable vehicle definition with collision bounds, mass, axles,
  suspension, steering, brakes, driver/passenger cameras and mirrors.
- Translate supported door pivots, wheel/steering/wiper animations and switches to the
  native model/script system, keeping helper geometry out of visible meshes.
- Read seat/standing and per-door path markers to build a passenger cabin and boarding
  paths. Counted mesh tags alone do not establish usable passenger capacity.
- Translate compatible engine/gearbox and sound configuration to native physics/scripts;
  Proton/Ciano scripts cannot simply be executed by the OMSI script runtime.
- Convert authored skins, material presets and texture switches without reintroducing
  washed-out reflections or invisible depth barriers.
- Define and test the vehicle conversion declaration/packaging policy before adding an
  installer entry point. Do not silently convert `.ini` files as native OMSI vehicles.

Test source packages remain outside the repository. No original vehicle, texture,
sound, third-party script or downloaded example is part of this contribution.
A synthetic fixture checks safe references, legacy recognition, door direction,
coordinate mapping and source preservation. Actual driving, animation, physics,
boarding, graphics performance and PBSU compatibility remain untested.

The primary [vehicle format reference](https://www.busmods.com/ciano/help/vehicle-overview.html)
and [doors/seats reference](https://www.busmods.com/ciano/help/vehicle-doors.html) describe
Ciano, an experimental project based on Proton. They guide preparation but do not prove
that every older PBSU mod has the same behavior.

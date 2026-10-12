# Proton Bus vehicles in OpenOMSI

The vehicle converter generates an experimental **drivable rigid bus**, using the existing
streaming 3DS/material/texture converter and OpenOMSI's native vehicle simulation. It does
not execute Proton/Ciano C# scripts. Map conversion stays in the launcher's Mods flow and
keeps its existing mandatory declaration.

## Creator declaration

Append this extension of ours to the original vehicle main `.bus.txt` or legacy `.ini`:

```ini
[openomsi]
allowConversion=1
```

Exactly one canonical field with the literal value `1` is required. Missing, empty,
invalid, disabled, duplicated or conflicting fields block the CLI and public conversion
API **before any output is created**. No bypass or automatic insertion exists. The copied
main file is checked again in staging. Failed or cancelled conversion removes the staging
package, and publication happens only after every generated file succeeds. Source files,
credits and licenses are preserved byte for byte in `Source/` in the resulting vehicle.
Use a dedicated unpacked source folder, not a downloads directory holding unrelated files.
The package source limit is 2 GiB, generated geometry 512 MiB, skins 256 MiB; individual
3DS files retain the existing 1 GiB reader limit. Configurations and object counts are bounded.

The field is editable text, **not proof of identity, verified authorization, copy protection
or permission to redistribute**. Public availability is not a redistribution grant.
The extra section's compatibility with PBSU has **not been tested in PBSU**. It is our
extension, appended at the end; original version, folder and physics fields are not changed.

## Convert and install (Windows)

```powershell
cargo build --release -p omsi-proton --example import_vehicle
.\target\release\examples\import_vehicle.exe "C:\BusSource\Example.ini" "C:\ConvertedBus"
```

Modern configurations can use `Example.bus.txt` instead. The output folder must be new
and outside the original source package. The CLI prints a JSON report of meshes, places,
doors, sounds and limitations. On Linux/macOS:

```sh
cargo run --release -p omsi-proton --example import_vehicle -- /bus-source/Example.bus.txt /converted-bus
```

Install `ConvertedBus` through **Mods → Extract**, or put its `Vehicles` folder into
OpenOMSI's content folder, preserving the structure. Select the bus under manufacturer
`Proton Bus` in the ordinary vehicle picker. Source `.ini` files are never offered as
native OMSI definitions.

Generated files are `.busoi`, `.cfgoi` and `.o3doi`: versioned compressed OI envelopes,
read by OpenOMSI only. Renaming them to `.bus`/`.cfg`/`.o3d` does not decode them. This is
an interoperable open-source format, **not DRM** or a guarantee against reconversion.
Textures/sounds/skins and attribution keep their original formats and bytes.

## Driving and supported systems

- Rigid templates `4x2`, `6x2`, `6x4`: mass, collision box, centre of gravity,
  authored wheel locations/radii, spring/damper rates, steering angle, driven axles.
  Body inertia is approximated from a uniform collision box about the authored centre
  of mass, so rigid dynamics do not inherit the native near-zero default.
- Native automatic drive with authored forward/reverse ratios, differential ratio,
  bounded engine torque/RPM, shift hysteresis, brake and parking-brake force. This is an
  approximation, not a port of the source torque converter, tyre model or engine dynamics.
- Driver/passenger cameras and reflection-camera definitions; tagged wheels spin, steer
  and follow suspension; steering wheel, speedometer, rev counter and hinged door leaves
  use native animations. Mirrors and lamps require in-game visual validation.
- Independent clickable door buttons. `bus_doorfront0`, `bus_doorfront1`, etc. address
  the physical doors in inspection order; `bus_dooraft` toggles the rear doors together,
  and `bus_dooraftclose` closes them. Opening is refused while moving; open doors prevent
  drive torque, and a doorway occupied by a passenger keeps the door open. Native `PAX_Entry/Exit` states follow the door's actual opening.
- Seat/standing markers (or explicit seats file), original contiguous door paths and
  coincident shared nodes become native passenger places/paths. Missing routes omit that
  portal with a warning; no aisle links are guessed through seats. Native humans must
  be installed. Turnstile payment and source teleport/release flags are approximated.
- Skin slots/repaints (one shared texture per source image, including repeated body/door slots), transparent/cutout materials, gauges, dashboard switches,
  indicators, headlights/reverse lamps and interior lights; authored frame-based wipers.
- Authored engine, horn and door-air sound loops with RPM/volume curves, camera masks and gear/throttle
  conditions. Source-specific pitch curves, retarder/turbo/filter semantics remain approximate.
- Native shared HOF selection and textual destination/line displays use installed
  `Annax Small`/`Annax Large` fonts on recognized display tags. Source destination images,
  scrolling displays and custom dashboard scripts are not translated yet. No fonts are copied.

Start the engine (`kw_m_enginestart`, default M), select D (`automatic_D`, default D),
release the parking brake (`parking_brake_toggle`, default numpad decimal), and use the
normal OpenOMSI steering/throttle/brake bindings or your configured controller. R and N
select reverse/neutral; direction changes are refused while moving. Lights, indicators
and wiper commands use ordinary native bindings. The bus starts with electrics on,
engine off, neutral selected and parking brake applied.

Articulated meshes/parts and offloaded script animations are explicitly rejected. Manual
gear triggers select a ratio in the native automatic approximation; there is no accurate
Proton manual clutch simulation. Ramps, custom suspension cabins, wet-glass masks, custom
vehicle-script logic and some tag/control variants still require additional translators.
The converter reports these limits instead of claiming complete PBSU runtime equivalence.

## Inspection and validation

The read-only inspector is still available independently of conversion:

```powershell
cargo build --release -p omsi-proton --example inspect_vehicle
.\target\release\examples\inspect_vehicle.exe "C:\BusSource\Example.ini"
```

It writes only JSON to stdout. It does not insert permission, mutate a source, generate a
bus or bypass conversion checks. Synthetic tests cover accepted/denied declarations,
rollback after failure/cancellation, attribution preservation, OI/native loading, geometry
across local zero, selection, acceleration/steering/braking/reverse and passenger-door state.
No original vehicle/model/texture/sound/script or downloaded example is committed.
Actual bus graphics, mirrors, handling, animation pivots, passenger clearance, audio mix,
performance and PBSU compatibility need runtime testing on the user's machine.

The primary [models/tags reference](https://www.busmods.com/ciano/help/vehicle-models.html)
and [doors/seats reference](https://www.busmods.com/ciano/help/vehicle-doors.html) describe
Ciano, an experimental project based on Proton. They guide supported translations, not a
claim that every legacy PBSU mod behaves identically.

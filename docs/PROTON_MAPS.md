# Basic Proton Bus map support

The Mods installer can import a standalone Proton Bus map ZIP or folder containing
`<name>.map.txt` next to its `<name>/` folder. Phases 2, 3 and 4 are accepted for **free
driving with an OMSI bus**.

## Install and drive

1. Open **Mods**, select the map ZIP, and install with **Extract** (or **Auto** when
   there is enough free space). For an unpacked map, select the folder containing
   both the `.map.txt` and its base folder.
2. Read the import report, especially any missing textures.
3. On **Drive**, select the map labelled **(Proton Bus)**, choose an installed OMSI
   bus and one of the map's entry points, then start free driving without a duty.

Importing creates a separate `maps/Proton-<name>` map. Original OMSI maps are not
converted. The source archive stays untouched, and its assets are not distributed
with openOMSI. Imported maps still use openOMSI's normal OMSI 2 installation setup.

## Included

- Static `.3ds` scenery, UV coordinates, materials and referenced textures.
- PNG cutouts and explicit 3DS material opacity; custom Proton shaders are not reproduced.
- Explicit entry points from the base folder or selected model folder. Names and
  coordinates are preserved; the importer does not invent route numbers or duties.
  An entry point without a nearby supporting face is moved to the closest reasonably
  flat static face, with a warning to check its position and clearance in game.
- Scenery divided into 300 m tiles, with clipped geometry at tile boundaries and
  a driveable surface using the existing scene loader.
- Simulation helper geometry identified by the map's pedestrian, traffic, stop and
  traffic-light prefixes is left out rather than drawn as obstacles.

## Limits of this first version

Passengers, pedestrian paths, AI vehicle paths, working traffic lights, bus routes,
timetables, destination graphics and Proton buses are **not imported**. This is a
scenery import, not full Proton Bus compatibility. Animated models, prefabs, tile
streaming rules and texture `.config.txt` shader settings are not reproduced.

The map's static visible mesh supplies the wheel surface. Collision and appearance
need checking in game: foliage, roofs, glass and author-specific material settings
may need further adaptation. A missing texture is reported and its material is
kept untextured; it does not prevent the entire map from loading.

Archives used **in place** are not supported for Proton maps because conversion
creates new files. The installer reserves additional disk space for those files and
converts in its staging area before moving the result into the content folder.
Cancellation or a failed conversion does not commit the staging result.

The basic standalone layout requires the base folder to have the same name as its
`.map.txt` file. Unsupported map phases, unsafe paths, damaged 3DS records and maps
without an explicit entry point produce an import error.

## Local format checks

Synthetic tests run with `cargo test -p omsi-proton`. Real maps must stay outside the
repository. A developer can extract one into a fresh temporary folder and run:

```sh
PROTON_MAP_FIXTURE=/path/to/freshly-extracted-map \
PROTON_MAP_NAME=LocalTest \
cargo test -p omsi-proton supplied_map_fixture_when_available -- --nocapture
```

This converts that temporary folder and checks generated maps and meshes with the
engine's format loaders. It is **not** a GPU, driving or performance test. Use a
fresh extraction for each run; imported output is reserved and is not overwritten.

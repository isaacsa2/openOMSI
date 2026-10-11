# Basic Proton Bus map support

The Mods installer can import a standalone Proton Bus map ZIP or folder containing
`<name>.map.txt` next to its base folder, or next to `tiles/` when `baseDir=.`.
Phases 2, 3 and 4 are accepted for **free
driving with an OMSI bus**.

## Install and drive

1. Open **Mods**, select the map ZIP, and install with **Extract** (or **Auto** when
   there is enough free space). For an unpacked map, select the folder containing
   both the `.map.txt` and its base folder.
2. Read the import report, especially any missing textures.
3. On **Drive**, select the map labelled **(Proton Bus)**, choose an installed OMSI
   bus and one of the map's entry points, then start free driving without a duty.

To apply conversion improvements, remove the previously imported map through Mods
and install its original ZIP again with the updated build. Updating the executable
alone does not update generated O3D files.

Importing creates a separate `maps/Proton-<name>` map. Original OMSI maps are not
converted. The source archive stays untouched, and its assets are not distributed
with openOMSI. Imported maps still use openOMSI's normal OMSI 2 installation setup.

## Included

- Static `.3ds` scenery, UV coordinates, materials and referenced textures.
- Diffuse UV scale, offset and rotation, specular colour/strength, basic self
  illumination and shininess approximated as a Phong exponent. Material definitions
  may follow their objects. UTF-8 names and legacy Windows-1252 names are accepted.
- PNG cutouts and explicit 3DS material opacity; custom Proton shaders are not reproduced.
- Explicit entry points from the base folder or selected model folder. Names and
  coordinates are preserved; the importer does not invent route numbers or duties.
  An entry point without a nearby supporting face is moved to the closest reasonably
  flat static face, with a warning to check its position and clearance in game.
- Scenery divided into 300 m tiles, with clipped geometry at tile boundaries and
  a driveable surface using the existing scene loader.
- Identical vertices are shared within each tile, keeping distinct UV seams and
  normals. Each tile exports only the materials it uses; identical material slots
  are shared and textures referenced only by discarded helpers are not copied.
- GPS helper files and objects with a separate `gps` name token are left out,
  including the coloured navigation strips baked into some maps.
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

Conversion scans material headers first, then reads one mesh object at a time.
Clipped faces are spooled to temporary files in staging using at most 32 buffered
writers. Each tile is then assembled with shared vertices and exported separately;
its temporary file is deleted immediately. The whole converted map is not retained
in RAM. Peak memory still depends on the largest source object and densest output
tile, plus bounded lookup buffers. Extra disk I/O can make an import slower.

The installer first looks for a base folder matching the `.map.txt` name. It also
accepts a single neighbouring base folder containing `tiles/`, or a single config
alongside `tiles/`. Ambiguous folders are not guessed. The config's `baseDir` must
refer to the selected layout; relative `./` prefixes and trailing separators work.
Unsupported map phases, unsafe paths, damaged 3DS records and maps
without an explicit entry point produce an import error.

The importer has no fixed ZIP size cap: a 1 GB archive is accepted when its format,
conversion limits and available disk space allow it.

Generated geometry has an **8 GiB** budget for vertex and triangle data across all
tiles and source files, including faces produced by clipping, before compaction. This replaces the
earlier vertex-count limits. It is not a limit on the ZIP size or total process RAM:
source meshes, vector capacity, textures and other conversion data use extra memory.
Each source 3DS file may be up to **1 GiB**, and a map may produce up to **16,384 tiles**.
The separate bound of 4096 tiles spanned by a single triangle still applies.

Bytes after a 3DS file's declared main chunk are skipped without loading them into
memory, and reported in the import warnings. Invalid or truncated chunks inside that
main chunk still stop conversion. Large-map gameplay and RAM use need checking on
the machine that will run the map; the import budget is not a gameplay memory budget.
Camera/light objects are skipped, and keyframe data produces a warning that only
the static mesh pose is imported. The local-axis matrix is not reapplied to vertices
that already contain the exported world transform.

The [PBSU Blender exporter](https://github.com/Arturo254/PBSU-Addon-3ds) was examined
as a format reference; its Python implementation is not included in this importer.

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

To measure the conversion alone, without loading the result into the renderer:

```sh
cargo build --release -p omsi-proton --example import_map
target/release/examples/import_map /path/to/freshly-extracted-map LocalTest
```

Local Linux measurements before/after the streaming implementation used the same
release profile, a separate process per import and the median of three fresh
extractions per map. ZIP extraction and gameplay were excluded. Peak resident RAM
was recorded with `wait4`; these are not Windows, GPU or 1 GB map benchmarks.

| Local map | Peak RAM before → after | Conversion time before → after |
| --- | --- | --- |
| Circular 606 | 153.6 → 47.1 MiB | 0.674 → 0.696 s |
| Goianira 361 | 42.1 → 29.4 MiB | 0.206 → 0.241 s |
| Cajueiro Seco | 28.4 → 13.9 MiB | 0.135 → 0.127 s |
| Osasco Lite | 78.6 → 15.2 MiB | 0.770 → 0.784 s |
| Osasco Full | 83.1 → 15.2 MiB | 0.469 → 0.591 s |

All five maps kept byte-identical vertex attributes, triangle indices, entry points
and terrain. Material records gained the additional supported 3DS properties.

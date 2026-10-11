# Basic Proton Bus map support

The Mods installer can import a standalone Proton Bus map ZIP or folder containing
`<name>.map.txt` with its assets under the relative `baseDir`, including nested folders,
or next to `tiles/` when `baseDir=.`. Packs with multiple configs install each map separately.
Phases 2 through 4, including 3.1, are accepted for **free
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
- `_transparent_` mesh names select alpha blending; `_cutout_` selects alpha testing.
  Materials with different name tags remain distinct even when they share one texture.
  Untagged PNGs retain the cutout fallback, and explicit 3DS material opacity is blended.
  Other custom Proton shaders are not reproduced.
- `_gencol_` + `_invisible_` meshes become separate collision-only objects: they support
  wheels and vehicle collision without render meshes, textures or shadows. Untagged
  scenery retains the initial importer's wheel-surface behaviour.
- `_low_speed_zone_`, `_force_exit_`, `_spawn_blocker_` and non-colliding `_invisible_`
  helpers are omitted and reported. `_utree###_` boxes are omitted with a warning:
  they request built-in Proton trees, which are not included with openOMSI.
  Name tags are case-sensitive, following the [loader documentation](https://www.busmods.com/ciano/help/map-models.html).
- Explicit entry points from the base folder or selected model folder. Names and
  coordinates are preserved; the importer does not invent route numbers or duties.
  An entry point without a nearby supporting face is moved to the closest reasonably
  flat static face, with a warning to check its position and clearance in game.
- Scenery divided into 300 m tiles, with clipped geometry at tile boundaries and
  a driveable surface using the existing scene loader.
- Identical vertices are shared within each tile, keeping distinct UV seams and
  normals. Each tile exports only the materials it uses; identical material slots
  are shared and textures referenced only by discarded helpers are not copied.
- Material colours, opacity, specular and self-illumination stored in the 3DS are
  ignored, as in Proton's model loader: exporter defaults must not tint textures
  or add a broad white reflection. Texture alpha and supported mesh-name tags
  determine transparency. Reimport the original ZIP to update existing conversions.
- Authored GPS strips are kept as a lightweight navigation overlay, selected by
  the entry-point name (`_gps_` plus `_<name>_`). They appear on both the small
  navigator and city map, without visible scenery, collision or source materials.
  GPS-only files are also recognised; unmatched strips are omitted and reported.
- Vehicle-path coordinates in `aivehicles/*.txt`, or exact `<prefix>.000` 3DS
  marker names, become roads on the navigator and launcher map. Numeric ordering,
  reversal and complete loops are preserved; missing point numbers split the road
  instead of connecting across a gap. The displayed width is an approximate 3.5 m.
- `streetlights/*.txt` prefixes and `_real_` / `_fake_` markers become native
  point lights and glow sprites at their mesh bounds' centre. Real-light colour,
  intensity and range, and glow texture, tint and size are converted. They use the
  engine's night switch and its nearby-light filtering; marker meshes are omitted.
  Glow textures are shared by filename. Missing glows fall back to the native glow.
- Simulation helper geometry identified by the map's pedestrian, traffic, stop and
  traffic-light prefixes is left out rather than drawn as obstacles.

## Limits of this first version

Passengers, pedestrian simulation, AI vehicle simulation, working traffic lights,
bus timetables, destination graphics and Proton buses are **not imported**. This is a
scenery import, not full Proton Bus compatibility. Animated models, prefabs, tile
streaming rules and texture `.config.txt` shader settings are not reproduced.

The GPS follows the author's drawn strip; it does not recalculate routes, announce
turns or create a timetable. Vehicle paths supply navigation only and do not start
AI traffic or generate road surfaces: the original 3DS road/collision geometry
still supplies the drivable surface. Manual path links and spline smoothing are
not reproduced. The tag/point conventions follow the public
[map model](https://www.busmods.com/ciano/help/map-models.html),
[path](https://www.busmods.com/ciano/help/map-traffic.html) and
[streetlight](https://www.busmods.com/ciano/help/map-lights.html) references.

Streetlight rendering is approximate. Imported point lights follow the native
night cycle and do not allocate additional lamp shadow maps; source shadow options
are not reproduced. `alwaysOn` is supported for glow sprites; real lights still
switch with night, with a warning for such definitions. Fixed glow rotations and
nonuniform scale become billboards; fixture mesh emission is not animated at night.
Streetlight markers outside tiles with scenery are omitted and reported.

Navigation is stored separately in a versioned `proton-import/navigation.json`,
limited to 250,000 GPS triangles and 250,000 road points (up to 4,096 routes/roads),
with a 64 MiB file limit. GPS geometry never enters the 3D scene's buffers. Nearby
GPS triangles are cached around the small map's anchor; the city map caches the
selected route. A damaged or missing navigation file leaves scenery usable.
These limits apply to optional navigation metadata, not the source map archive.

The map's static visible mesh and explicit invisible colliders supply the wheel surface. Collision and appearance
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

The installer reads up to 64 KiB per `.map.txt` while planning and follows its authored
`baseDir`, preserving nested paths and resolving folder names case-insensitively. Each
map receives only that base folder and its own config. Without an available `baseDir`,
the earlier matching-name/single-neighbour detection remains available. Ambiguous
folders are not guessed. Relative `./` prefixes and trailing separators work.
Unsupported map phases, unsafe paths, damaged 3DS records and maps
without an explicit entry point produce an import error.

The importer has no fixed ZIP size cap: a 1 GB archive is accepted when its format,
conversion limits and available disk space allow it.

Generated geometry has an **8 GiB** budget for vertex and triangle data across all
tiles and source files, including faces produced by clipping, before compaction. This replaces the
earlier vertex-count limits. It is not a limit on the ZIP size or total process RAM:
source meshes, vector capacity, textures and other conversion data use extra memory.
Each source 3DS file may be up to **1 GiB**, and a map may produce up to **16,384 tiles**.
Ordinary triangles may span at most 4096 tiles. Oversized, nearly horizontal backdrop
faces (height range at most 1 m) are deferred and clipped only to tiles occupied by
other scenery, with a warning; at most 256 such faces are accepted. This avoids turning
a large empty ground sheet into thousands of additional map tiles.

Bytes after a 3DS file's declared main chunk are skipped without loading them into
memory, and reported in the import warnings. Invalid or truncated chunks inside that
main chunk still stop conversion. Large-map gameplay and RAM use need checking on
the machine that will run the map; the import budget is not a gameplay memory budget.
Camera/light objects are skipped, and keyframe data produces a warning that only
the static mesh pose is imported. The local-axis matrix is not reapplied to vertices
that already contain the exported world transform.

The [PBSU Blender exporter](https://github.com/Arturo254/PBSU-Addon-3ds) was examined
as a format reference; its Python implementation is not included in this importer.

The supplied CSS pack (1,058,211,442-byte ZIP, 1,495,228,617 bytes expanded) includes
phase 3 and phase 4 configs pointing at separate nested base folders. Both were
installed from the complete ZIP through the launcher and checked with the engine's
map, terrain, scenery, O3D and geometry loaders. Each retains its nine entry points;
no missing textures were reported. Its roughly 25 km ground sheet is cropped to
occupied tiles rather than producing thousands of empty tiles. One Linux release
conversion run per version, excluding ZIP extraction and later loader validation:

| CSS version | Output tiles | Output triangles | Conversion peak RAM | Conversion time |
| --- | ---: | ---: | ---: | ---: |
| Phase 3 | 363 | 4,457,414 | 85.5 MiB | 5.18 s |
| Phase 4 | 365 | 30,792,230 | 196.0 MiB | 35.74 s |

These are conversion measurements, not gameplay RAM or FPS. The phase 4 map remains
very dense; its rendering, collision and driving still require an in-game test.

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

## GPU residency on dense maps

Generated maps use the sizes of their per-tile O3D files to estimate resident geometry
before loading it. The streamer selects a smaller radius where dense scenery would
exceed its geometry allowance: 30% of reported dedicated VRAM, bounded to 128–1500 MB,
or 384 MB when dedicated VRAM is unknown. The requested view distance remains the
maximum. Native OMSI maps keep their existing distance rules.

The estimate includes a 12.5% allowance and 64 KiB per tile for terrain and allocation
slack, and counts the 75 m unload margin. Every camera/player area retains at least
150 m (or the shorter requested distance). This is not a hard GPU memory limit: nearby
ground takes priority, one exceptionally dense tile may exceed the allowance, and
textures, vehicles, render targets and temporary uploads also use VRAM. Imported tiles
are prepared one at a time to avoid allocating a batch of six heavy tiles ahead of
placement. The effective radius and geometry allowance appear in the log.

Vertex sharing now rotates its bounded lookup when full, so later geometry in a dense
tile can still share identical positions, normals and UVs. No faces or detail are
removed. Reimport the source archive to regenerate these meshes; the streaming change
also applies to existing generated maps with the standard per-tile file layout.
In-game GPU usage, visual distance transitions and frame rate still need validation
on a graphics adapter; conversion RAM measurements above do not measure VRAM.

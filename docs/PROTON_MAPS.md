# Proton Bus map preview support

The Mods installer can import a standalone Proton Bus map ZIP or folder containing
`<name>.map.txt` with its assets under the relative `baseDir`, including nested folders,
or next to `tiles/` when `baseDir=.`. Packs with multiple configs install each map separately.
Phases 2 through 4, including 3.1, are accepted for **free
driving with an OMSI bus**.

## Declaração obrigatória / Required declaration

O conversor **continua no launcher**, pelo fluxo de Mods. Antes de importar, o criador
acrescenta ao **final do `.map.txt` original**, sem alterar `mapModVersion`, `baseDir`,
`modelsDir` ou os demais parâmetros:

```ini
[openomsi]
allowConversion=1
```

Essa seção é uma **extensão do OpenOMSI**, não um parâmetro oficial do PBSU.
Somente um campo `allowConversion` na seção `[openomsi]`, com valor literal `1`,
é aceito. Ausência, `0`, vazio, valor inválido, grafia diferente ou declaração
repetida/conflitante bloqueiam a conversão. Todos os mapas selecionados são verificados
antes da primeira conversão, e os arquivos copiados para staging são verificados
novamente. A CLI e a API pública usam a mesma regra. Não há opção para ignorar o
bloqueio, inserção automática da declaração ou alteração do original pelo programa.

O campo é texto editável, inclusive por terceiros: **não é prova de identidade,
autorização verificável, proteção contra cópia ou permissão de redistribuição**.
Créditos e licenças existentes são preservados; os direitos e condições do pacote
continuam sendo os do autor.

The creator appends the exact section above to the original `.map.txt`. This is our
extension, not an official PBSU setting. One canonical literal `1` is required;
missing, empty, invalid or duplicate/conflicting declarations block every conversion
entry point before converted files are generated. The launcher keeps its converter.
The editable field does not establish identity, verified authorization, copy protection
or redistribution rights. Source parameters and existing attribution remain unchanged.

**PBSU compatibility has not been tested in its runtime here.** Parser tests and public
loader references do not establish whether PBSU accepts this appended section. A creator
should check the original map in PBSU after adding it; that validation remains pending.

## CLI (Windows included)

The existing example uses the same `omsi-proton` implementation as Mods. From the repository:

```powershell
cargo build --release -p omsi-proton --example import_map
.\target\release\examples\import_map.exe "C:\Maps\Source" "C:\Maps\Converted" "Town"
```

On Linux/macOS:

```sh
cargo run --release -p omsi-proton --example import_map -- /maps/source /maps/converted Town
```

Choose a source folder with exactly one root `.map.txt` and an **absent** output folder
outside the source. The CLI copies the source, converts in temporary staging and publishes
the output only on success. It never writes to the original folder. For multiple maps or
ZIP input use Mods, which checks all selected configurations before conversion. Install
the resulting folder (containing `maps/` and `HOFs/`) via normal **Mods → Extract**;
it is already converted and is not converted again.

## OpenOMSI-only generated format

Generated engine files use a typed, versioned, compressed **OI** envelope and extensions
ending in `oi`: `global.cfgoi`, `.tileoi`, `.terrainoi`, `.oscoi`, `.o3doi`, `.hofoi`,
`.ovhoi` and `.jsonoi`. The header is 24 bytes: `OPENOMSI-OI` plus NUL (12 bytes),
little-endian version `u16` (1), file kind `u16`, uncompressed byte length `u64`, followed
by zlib data. File kinds are config=1, tile=2, scenery=3, mesh=4, terrain=5, depot=6,
navigation=7 and vehicle=8. Readers reject wrong versions/types, lengths, oversized
payloads and trailing compressed data. Mesh payloads are limited to 1 GiB per file;
other kinds to 64 MiB. Normal OMSI definitions keep their original reader behavior.

The original OMSI does not read this generated format, and renaming extensions alone
does not decode it. This is a format distinction, **not DRM**: the format and converter
are open source. Original images, sounds, source configurations, credits and licenses
retain their filenames and bytes. Reimport an older conversion to obtain the OI package.
The auxiliary terrain is retained for CPU probes but omitted from rendering on OI tiles,
so it does not produce the distant green ground below the authored scene.

## Install and drive

1. Open **Mods**, select the map ZIP, and install with **Extract** (or **Auto** when
   there is enough free space). For an unpacked map, select the folder containing
   both the `.map.txt` and its base folder.
2. Read the import report, especially any missing textures.
3. On **Drive**, select the map labelled **(Proton Bus)**, choose an installed OMSI
   bus and one of the map's entry points, then start free driving without a duty.
4. Use the generated **<map> (Proton Bus)** depot and set a normal destination in
   the destination menu (Shift+U). Stop close to the platform and open the matching
   doors. Boarding requires installed native human models and a bus passenger cabin.

To apply conversion improvements, remove the previously imported map through Mods
and install its original ZIP again with the updated build. Updating the executable
alone does not update generated O3D files.

Importing creates a separate `maps/Proton-<name>` map. Original OMSI maps are not
converted. The source archive stays untouched, and its assets are not distributed
with openOMSI. Imported maps still use openOMSI's normal OMSI 2 installation setup.

## Included

- Static `.3ds` scenery, UV coordinates, materials and referenced textures.
- Diffuse UV scale, offset and rotation; conservative native material equivalents. Material definitions
  may follow their objects. UTF-8 names and legacy Windows-1252 names are accepted.
- `_transparent_` mesh names select alpha blending; `_cutout_` selects alpha testing.
  Materials with different name tags remain distinct even when they share one texture.
  Untagged PNGs retain the cutout fallback.
  `_emissive_` meshes are self-lit, including combined transparent/cutout tags.
  `_additive_` is approximated by self-lit alpha blending without depth writes;
  this is not exact additive blending. Common texture `.config.txt` presets
  (`automotive/basicwhite`, asphalt, concrete and pavements) receive conservative
  native specular values. `keepTexture=0` retains the local texture and warns because
  built-in Proton replacement textures are unavailable. Other shaders are not reproduced.
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
  marker names, become roads on the navigator and launcher map, plus native traffic paths. Numeric ordering,
  reversal and complete loops are preserved; missing point numbers split the road
  instead of connecting across a gap. The displayed width is an approximate 3.5 m.
  Vertex-only 3DS objects are retained as markers; they need no faces and produce
  no visible helper geometry. The same applies to authored pedestrian points.
- `streetlights/*.txt` prefixes and `_real_` / `_fake_` markers become native
  point lights and glow sprites at their mesh bounds' centre. Real-light colour,
  intensity and range, and glow texture, tint and size are converted. They use the
  engine's night switch and its nearby-light filtering; marker meshes are omitted.
  Glow textures are shared by filename. Missing glows fall back to the native glow.
- Automatic vehicle and `aipeople` paths become tile-local native street/sidewalk
  segments, keeping slopes, direction, complete loops, point gaps and turn indicators.
  Speed multipliers become native speed limits based on a 50 km/h preview default.
  Vehicle and bus spawner intervals are approximated by native traffic density; a point
  with both spawners disabled has zero density. Bus-only points exclude ordinary cars;
  they are not exact timed spawners.
- `busstops` coordinates or exact trigger/waiting 3D markers become native stops and
  waiting places, including stop names, platform side and a bounded passenger count.
  Coordinate waiting positions stay in world space; only their facing is relative
  to the stop. This prevents a stop's position from being applied twice.
  A 3D stop faces the nearest authored vehicle path when one exists. Missing triggers
  are omitted and reported; missing waiting markers receive a small approximate
  waiting area with a warning. Use an installed OMSI bus with a passenger cabin,
  doors and a destination set to board in free drive. Existing native boarding,
  ticket and disembarking logic is used; no fictional line or timetable is generated.
- A basic textual HOF lists entrypoint names, `dest/` folder names and imported stop
  names. Exact duplicate destination names are combined. `outofservice.txt` makes a
  destination unload passengers and refuse boarding. Stops explicitly marked
  `isLatestStop=1` also become selectable terminal destinations: choosing the stop's
  own destination makes the native passenger logic unload there. No entrypoint is
  guessed to serve a particular terminal. Codes are generated indices, not original
  line numbers or IBIS trips. The HOF is shared through `HOFs/`, selected for the map
  by default and recorded with the mod for removal; vehicle folders are not edited.
  Explicit `ailists.cfg` depot choices take precedence. This supports text displays;
  source destination images and native roll-sign bitmaps are not converted.
- `trafficlights` cycles become native programs. Lamp markers accept both
  `<prefix>_pathN_...` and `_<prefix>_pathN_...`, including exporter numeric suffixes.
  Red/yellow/green channels and stop-trigger channels remain independent. Lamps
  and glows follow the program, and matching vehicle paths are split at stop lines.
  Decorative/flashing lights without an authored stop trigger do not hold traffic.
- Simulation helper geometry identified by the map's pedestrian, traffic, stop and
  traffic-light prefixes is left out rather than drawn as obstacles.

## Current limits

Bus timetables, destination graphics and driveable Proton bus mods are **not imported**.
This is partial compatibility. Animated models and source tile streaming
rules are not reproduced. Authored `prefabs.txt` instances and `prefabs/*.txt` definitions
are converted through the existing scene converter, preserving position, Unity Euler
rotation, custom nonuniform scale and mirrored winding. Missing prefab assets warn and
are omitted. Built-in Proton trees and random vegetation placement are not reproduced. `isFirstStop`/`isLatestStop` do not create native duties;
terminal behaviour uses the native bus destination and passenger logic.
Textual HOF destinations do not add announcements, ordered stop sequences or duties.
The Ciano references describe [bus stops](https://www.busmods.com/ciano/help/map-busstops.html)
and [destination displays](https://www.busmods.com/ciano/help/displays.html); a Proton
display image is not a textual HOF or timetable.

Source `aicars_v1` and `aibuses_v1` definitions import static 3DS models with the same
UV/material/texture converter, `_skin001_` paint slots, bounded PNG random skins,
weights, masses, geometry bounds, authored speed caps and simple moving/stopped sound
loops. They run as native AI road vehicles, **not drivable Proton buses**. Source
wheel/suspension/door animation, engine physics, collisions from custom vehicle
colliders, passenger cabins, exact sound pitch and schedules are not reproduced.
A malformed supplied AI definition fails the staged import; missing models warn and
fall back when the category has no usable authored models.

Missing car/bus categories use up to eight vehicle types referenced by the installed
**Grundorf `ailists.cfg`**, including depot bus type groups. Stock assets are reused
from the installation, not bundled or copied into the converted package. If source
cars exist, `disableNativeTraffic=1` suppresses the native car supplement; missing source
cars still allow the requested fallback. Missing Grundorf or its models logs a clear
warning and leaves scenery usable. An explicit map `ailists.cfg` overrides the fallback,
including an empty list. Bus models need authored bus-allowed paths. Native human models
remain required for pedestrians/boarding; Proton pedestrian spawn intervals are not
reproduced. Paths need a supporting road/collision surface. Manual path links,
train paths and Proton spline smoothing remain unsupported and are reported rather
than invented; `automatic_setup.enabled=0` does not create an inferred traffic route.

The GPS follows the author's drawn strip; it does not recalculate routes, announce
turns or create a timetable. Format conventions follow the public
[map model](https://www.busmods.com/ciano/help/map-models.html),
[path](https://www.busmods.com/ciano/help/map-traffic.html) and
[lights](https://www.busmods.com/ciano/help/map-lights.html) references.

Signal clocks use native game time; random/first-tick startup timing is approximate.
Mesh-shaped stop triggers use bounded horizontal bounds. A normal trigger attaches
to the closest unambiguous authored vehicle path; `_anyangle_` can affect multiple
paths. Triggers without a matching path or with ambiguous matches are reported and
left unlinked. Signal lamps emit native glows; coloured real-light range/shadows are
not imported. Native path linking uses the engine's endpoint tolerance, so very close
but separate source endpoints should be checked in game.

Streetlight rendering is approximate. Imported point lights follow the native
night cycle and do not allocate additional lamp shadow maps; source shadow options
are not reproduced. `alwaysOn` is supported for glow sprites; real lights still
switch with night, with a warning for such definitions. Fixed glow rotations and
nonuniform scale become billboards; fixture mesh emission is not animated at night.
Streetlight markers outside tiles with scenery are omitted and reported.

Navigation is stored separately in a versioned `proton-import/navigation.jsonoi`,
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
its temporary file is deleted immediately. Gameplay keeps bounded centres/points,
not full helper meshes; lamp meshes are exported one at a time. A spatial tile index
limits stop/signal matching, and waiting-place records append directly to UTF-16 tile
files rather than rewriting each tile for every passenger position. The whole converted map is not retained
in RAM. Peak memory still depends on the largest source object and densest output
tile, plus bounded lookup buffers. Extra disk I/O can make an import slower.

The installer reads up to 64 KiB per `.map.txt` while planning and follows its authored
`baseDir`, preserving nested paths and resolving folder names case-insensitively. Each
map receives that base folder, its own config and recognized package-level credits,
licenses, copyright/readme/author files or folders. Without an available `baseDir`,
the earlier matching-name/single-neighbour detection remains available. Ambiguous
folders are not guessed. Relative `./` prefixes and trailing separators work.
Unsupported map phases, unsafe paths, damaged 3DS records and maps
without an explicit entry point produce an import error.
Entry-point `rotY` values with a trailing `d`, `D` or degree symbol are read as
degrees and reported in the import warnings. Other malformed entry-point numbers
remain errors, now identifying the file, section, field and original value.
The map importer replaces non-finite vertex UVs with `(0, 0)` and reports the file,
object and repaired count. Finite UVs, geometry and collision are preserved;
affected texture mapping needs checking in game. Positions, face indices and
chunk lengths remain strictly validated. The general 3DS readers remain strict.
Object-read errors identify the 3DS object.

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

The expanded phase 4 conversion was also checked with the native map/scenery/lane
readers: 142 stops, 191 sidewalk segments, 7 signal programs, 137 controlled lanes
and 461 lamp objects. The native O3D reader validated all 365 tile meshes and
30,787,338 static triangles. Assets remained outside the repository.

These measurements describe the earlier scenery-only conversion, not gameplay RAM or FPS. The phase 4 map remains
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

## Validation boundaries

Synthetic fixtures cover the permission gate (accepted/refused/absent/conflicting),
all-selected-map preflight, copied-config revalidation, source preservation, staging
cleanup after conversion failure, attribution bytes, OI references and decoding,
prefab transforms and source AI export. No original map or stock asset is committed.
Real user archives without the declaration are intentionally blocked; the converter
does not add it to run a test. PBSU loading, a real Windows CLI execution, GPU memory,
night lighting and AI/boarding behavior in-game still require runtime validation.

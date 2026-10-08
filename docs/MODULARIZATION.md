# Modularization plan

Splitting the large source files that mix responsibilities into cohesive modules, without
changing what the game does. This file is the plan, the architecture map and the progress
checklist. It is updated as each step lands.

## Rules for every step

* One responsibility per step, in its own branch and draft pull request. A step that builds
  on another is a chained pull request, based on the previous step's branch, so that its diff
  shows that step only.
* Code is **moved, not rewritten**: the moved lines stay byte-for-byte identical apart from
  visibility (`pub(super)`, `pub(crate)`) and the `use` lines a module needs. Every pull
  request shows how that was checked.
* Public interfaces stay where they are (a moved public item is re-exported from its old
  path), and so do file formats, protocols, shaders, serialization, initialization order,
  threads and synchronization.
* Global state (statics, atomics, environment variables, caches on disk) is moved with the
  code that owns it. Its effects and the order in which it is read and written stay as they
  are.
* No bug fixes, optimizations, features, dependency updates or reformatting in these steps.
  Problems found on the way are listed below for separate work.
* No module that only collects code without a responsibility of its own (`misc`, `utils`).

## Reference

| | |
| --- | --- |
| Upstream | `openOMSI-Project/openOMSI`, `main` at `c85b2e0dda13d219fd12d0496c653d20cdb54145` (0.2.16) |
| Fork | `isaacsa2/openOMSI`, `main` at the same commit |
| Toolchain used for the checks | rustc 1.99.0 on Linux x86_64 (`Cargo.lock` needs 1.97.1 or newer) |

Upstream is fetched again before every step (still `c85b2e0` at R2). The commit each step is based on is recorded in
its pull request.

## Baseline (upstream `c85b2e0`, before any step)

| Check | Result |
| --- | --- |
| `cargo test --locked --workspace --no-fail-fast` | 1317 passed, 0 failed, 36 ignored (they need a GPU, OMSI 2 content or the presence service) |
| `cargo build --locked --release` | passes, 4 warnings in `omsi-app` |
| `cargo fmt --all -- --check` | fails: 8155 hunks in 263 files (pre-existing; not fixed by these steps) |
| `git diff --check` | clean |

## Architecture map (largest files)

| File | Lines | What it holds today |
| --- | ---: | --- |
| `crates/omsi-render/src/lib.rs` | 15 830 | public scene types (camera, lighting, lights, materials, instances, scene); device capabilities and adapter memory; GPU arrays; bind group layouts; pipeline creation (`Renderer::new_on`); mesh, texture and material upload; bounds and culling; draw batching; exposure, sky light and fog maths; GPU timers; the frame (`render_inner`); surface handling |
| `crates/omsi-app/src/scene.rs` | 14 857 | world and object types, tile staging off the main thread, GPU cache, splines, lights, stop boards |
| `crates/omsi-app/src/traffic.rs` | 8 311 | AI cars, overtaking, junctions, traffic lights, parking, audio, LAN mirroring (one `impl Traffic` of about 6 000 lines) |
| `crates/omsi-app/src/schedule.rs` | 6 820 | timetables, trips, IBIS codes, blinds, the player's duty |
| `crates/omsi-app/src/humans.rs` | 6 754 | passengers, cabin and doors, pedestrian network, LAN avatars |
| `crates/omsi-sim/src/vehicle.rs` | 6 077 | vehicle type and instance, script host wiring, physics glue |
| `crates/omsi-app/src/input_script.rs` | 4 942 | key and controller input mapped to script triggers |
| `crates/omsi-app/src/lan.rs` | 3 939 | LAN session, messages, peers |
| `crates/omsi-app/src/app_events.rs` | 3 662 | the window event loop |

The `omsi-app` files are mapped in detail when their turn comes; the entries above are
starting points, not decisions.

## Plan

Order: `omsi-render` first, then the `omsi-app` subsystems, then the rest.

### omsi-render

| # | Step | Moves to | State |
| --- | --- | --- | --- |
| R1 | Shader source assembly (WGSL text and its adaptation to the device) | `shader_source.rs` | PR #168 |
| R2 | Device capabilities: backend, array path, feature flags, adapter memory, the per-adapter fallback file | `device_caps.rs` | PR #170 (on #168) |
| R3 | GPU arrays (storage buffer or texture) and their texture spans | `gpu_array.rs` | planned |
| R4 | Bind group layouts of the camera and material groups, with `sixteen_texture_units` | `layouts.rs` | planned |
| R5 | Mesh pages and mesh/texture preparation on worker threads | `mesh_pages.rs`, `prepare.rs` | planned |
| R6 | Draw batching and recording (`DrawItem`, `Batch`, `batch_items`, `encode_batches*`) | `batching.rs` | planned |
| R7 | Exposure, sky light and fog maths of the enhanced path | `sky_light.rs` | planned |
| R8 | GPU timers | `gpu_timers.rs` | planned |
| R9 | Public scene types (camera, lighting, lights, materials, instances, scene), re-exported from the crate root | `types/` | planned |
| R10 | `impl Renderer` split by responsibility (creation, resources, upload, frame) | `renderer/` | planned |

### omsi-app

| # | Step | State |
| --- | --- | --- |
| A1 | `scene.rs`: map, then tile staging, GPU cache, splines, lights, stop boards | planned |
| A2 | `traffic.rs`: map, then traffic lights, junctions, parking, audio, LAN mirroring | planned |
| A3 | `schedule.rs`: map, then IBIS codes, blinds, player duty | planned |
| A4 | `humans.rs`: map, then cabin and doors, pedestrian network, LAN avatars | planned |
| A5 | `input_script.rs`, `lan.rs`, `app_events.rs` | planned |

### Other crates

| # | Step | State |
| --- | --- | --- |
| S1 | `omsi-sim/src/vehicle.rs` | planned |

## Checklist

- [x] R1 shader source (PR #168)
- [x] R2 device capabilities (PR #170)
- [ ] R3 GPU arrays
- [ ] R4 bind group layouts
- [ ] R5 mesh pages and preparation
- [ ] R6 batching
- [ ] R7 sky light maths
- [ ] R8 GPU timers
- [ ] R9 public scene types
- [ ] R10 `impl Renderer` split
- [ ] A1–A5 `omsi-app`
- [ ] S1 `omsi-sim`

## Found on the way (separate work, not part of these steps)

* `cargo fmt --all -- --check` fails on upstream `main` in 263 files.
* `Cargo.lock` needs rustc 1.97.1 or newer (`reedsolomon-rs`); `docs/BUILDING.md` names no
  minimum Rust version.
* `snow_shader_source` and `corona_shader_source` repeat the same no-storage replacement.
* `RT_GBUF`'s comment ("the two above") points at `GBUF_FORMAT`/`AUX_FORMAT`, which stay in
  `lib.rs` after step R2.
* Off the lanes, the traffic asks every lane of the map for its traffic light each frame
  (`omsi-app/src/traffic.rs`, the depot gate requests); about 0.4 ms with 60 000 lanes.

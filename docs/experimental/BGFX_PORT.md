# Experimental bgfx renderer port (NOT READY TO MERGE)

## Current state

This branch is a **migration workspace**, not a completed openOMSI renderer.
Do not enable bgfx in the game, publish it as a playable release, or claim an
FPS improvement. The production renderer remains wgpu. The experiment lives
under `experiments/bgfx-port/`, in its **own Cargo workspace**, to avoid
introducing unverified native dependencies or changing the production lockfile.

### Implemented so far

- [x] Isolated `bgfx-sys` native API prototype with backend selection:
  Direct3D 11, Direct3D 12, Vulkan, OpenGL, Metal, or automatic.
- [x] bgfx initialization, one cleared view, empty frame submission,
  explicit shutdown, and invalid-argument checks.
- [x] Read-only migration inventory tool:
  `python scripts/audit-bgfx-port.py --output bgfx-inventory.json`.
- [x] Record unsupported features and merge criteria in one place.
- [x] A Win32 native-window prototype via winit 0.30 (not verified on a physical GPU).\n- [ ] Native window/swapchain handling on Linux, macOS, Android and iOS.
- [ ] A production `omsi-render` backend interface independent of wgpu types.
- [x] Load authentic .o3d via omsi-o3d; pack vertices/indices, upload bgfx buffers, and submit material runs with a normal-colour shader.\n- [ ] Full OMSI material/texture/lighting shaders and compatibility (not implemented).
- [ ] Interface, navigator, mirrors, previews and OpenXR rendered by bgfx.
- [x] Targeted source compilation and eight CPU-only tests on Windows and Linux.\n- [ ] Physical-GPU screenshot, parity, Android/iOS/macOS and performance tests.

**The Rust/native dependency source build and eight CPU-only unit tests have passed on Windows and Linux in GitHub Actions. No GPU run or game rendering has been verified yet.**
The prototype is unverified until it passes on a machine with Rust, a C++
compiler and graphics drivers. A successful clear-only test is not game parity.

## Building / trying the isolated prototype

Requirements: Rust toolchain, C++ compiler supported by the bundled bgfx
source, SDK/toolchain for the selected GPU API. The C bindings used by this
initial proof of concept are `bgfx-sys = 0.15.0`; evaluate binding freshness
and maintenance before integrating with the main game.

From the repository root:

```powershell
python scripts/audit-bgfx-port.py --output bgfx-inventory.json
cargo test --manifest-path experiments/bgfx-port/Cargo.toml
cargo run --manifest-path experiments/bgfx-port/Cargo.toml -- --backend dx11 --frames 120
cargo run --manifest-path experiments/bgfx-port/Cargo.toml -- --backend dx12 --frames 120
cargo run --manifest-path experiments/bgfx-port/Cargo.toml -- --backend vulkan --frames 120
cargo run --manifest-path experiments/bgfx-port/Cargo.toml -- --backend gl --frames 120
```

The CLI uses a **headless** surface (no native window handle). Some drivers
cannot initialize such a context. Failure is inconclusive; success proves only
that the binding and native backend initialized and accepted clear commands.
Its `cpu_submission_ms_per_frame` is NOT game FPS, GPU duration, swapchain
latency or draw throughput. Never compare it with openOMSI's FPS logs.

## Where the existing game depends on wgpu

This is a minimum map; generate current precise counts using the inventory:

| Subsystem | Major source locations | Work required |
| --- | --- | --- |
| 3D graphics | `crates/omsi-render` | Geometry, batching, pipeline state, shadow/depth passes, resources, texture compression, render targets, culling, shaders |
| Game loop | `crates/omsi-app` | Surface lifetime, device recovery, frame pacing, GPU upload, capture, overlays, windows, screenshot |
| GUI / launcher preview | `crates/omsi-ui`, `crates/omsi-plugin-ui`, launcher service | Fonts, UI meshes, atlas updates, render-to-texture, map/bus previews |
| Navigation | `crates/omsi-navigator`, `crates/omsi-mapdraw` | Overlay passes, map rendering, route drawing |
| VR | `crates/omsi-xr` | OpenXR graphics binding, DX12 device sharing and stereo frame timing |
| Photo | `crates/omsi-photo` | Intermediate attachments, temporal samples, compositing |
| Platforms | `apple/`, Android scripts, Linux/Windows | Native surface, resume/suspend, runtime libraries, toolchains |

The source currently exposes wgpu-specific handles and texture/pipeline
types across crate boundaries. A one-line Cargo dependency swap is not
possible. Replace public GPU types with a backend-neutral facade first,
then adapt each subsystem behind that facade. Preserve game simulation,
OMSI file parsing and saved-game/network protocols unchanged.

## Mandatory feature parity / known migration hazards

- **Shader pipeline:** wgpu WGSL/Naga sources cannot be passed straight to
  bgfx. Compile and verify backend-specific binaries (DX11, DX12, Vulkan,
  OpenGL/GLES, Metal) with a reproducible shader toolchain. Preserve lighting
  parity in Vanilla/Vanilla+, transparent/transmap depth semantics, headlamp
  shadows, glass, rain, clouds, night exposure, skinned animation and UI.
- **Draw ordering:** bgfx view sorting is not equivalent to existing render
  passes. Explicitly preserve authored transparent draw ordering, terrain
  masks, prepasses, mirrors, cab glass and shadow caster ordering. A screen
  that renders *something* is not proof that materials are correct.
- **Texture/model compatibility:** DDS/BC formats, unusual OMSI texture
  formats, recoloring, mipmaps, GPU memory pressure, articulated and
  biarticulated buses, script texture updates, 32-bit indices, decals,
  foliage and scenery must stay correct.
- **Graphics API support:** bgfx offers native DX11 (not wgpu's ANGLE/DX11
  route), DX12, Vulkan, OpenGL and Metal, but capabilities differ by
  hardware and platform. API availability is not proof of usable performance.
- **Threading/synchronization:** native bgfx init/frame uses global API
  thread constraints. Do not call it from arbitrary Rayon workers. Audit
  the existing encoding pool, sim thread, 2 frames-in-flight and GPU poll
  thread; compare frame latency and stalls with instrumented real frames.
- **Swapchain/device loss:** winit handles, resize/fullscreen, alt-tab,
  multi-monitor, VR targets, graphics-device-loss recovery and app shutdown
  need platform-specific implementations and repeatable tests.
- **Platform packaging:** Android arm64, Windows x64/ARM64, macOS
  x64/arm64, Linux x64/arm64, iOS, and their launchers/build scripts must
  ship the appropriate bgfx native libraries and shader variants.
  Rendering differences on PowerVR and older AMD GPUs are high risk.
- **Plugins and integrations:** OMSI content and plugins do not become
  portable because the renderer changes. Keep DirectInput, audio, network
  protocol, admin/API, save formats, input binding and gameplay unaffected.
- **VR/OpenXR:** currently shares wgpu DX12 devices. A bgfx graphics binding
  and texture interop strategy must be implemented and tested; until then
  VR cannot be declared supported by the port.

## Incremental implementation gates, all in this one draft PR

1. **Inventory**: scan the current main and identify direct and transitive
   wgpu dependencies. Baseline normal performance on matching map/vehicle
   and seed; capture `OMSI_PROFILE=1`, `OMSI_GPU_TIMERS=1` and
   `OMSI_DEBUG_DRAWS=1`.
2. **Platform**: create a real winit native window, initialize bgfx, report
   selected hardware backend, present and resize correctly. Separate
   DX11/DX12/Vulkan/GL/Metal checks. Never infer availability from headless.
3. **Backend facade**: abstract surface, texture, buffer, shader/program,
   material, pass, uniform, resource lifecycle and frame submission from
   wgpu so callers can choose one backend at runtime. Keep wgpu default
   until the bgfx path is complete and correctly fails/falls back.
4. **3D minimum**: render a textured OMSI `.o3d` bus mesh in a real scene
   using a valid bgfx shader set; then one stock map tile. Test screenshots.
5. **Complete scene**: terrain/splines, lighting, decals, transparent
   materials, animation, AI, particles, shadow/mirror, postprocessing,
   texture uploads, and streaming with instrumentation.
6. **Remaining surfaces**: cockpit/UI, launcher showroom/maps, navigator,
   photo, screenshot, triple screens, Android lifecycle and VR.
7. **Matrix**: build/test Windows, Linux, macOS, Android/iOS targets where
   supported, and Windows DX11/DX12/Vulkan/GL on both older and modern
   hardware. Compare matched scenes, drivers, resolution, traffic, seed,
   camera and render settings: median FPS, 1% low, CPU frame, GPU pass
   timings, stalls, VRAM, power when measurable, and correctness images.
8. **Release decision**: only remove wgpu after all required platforms and
   functionality pass. If even one required feature is missing, keep bgfx
   gated, mark this PR as draft and do not merge.

## Compatibility / non-goals

Do not copy copyrighted OMSI assets into this repository. Tests requiring
maps/buses load them from the tester's installed OMSI 2 directory. One
experimental PR does not relax AGPL source/license obligations or the
project's compatibility-first policy. Do not bundle code from another
project unless its license allows it and notices are preserved.

Do not run an automated benchmark on a different location, time of day,
bus, hardware or graphics settings and call it an A/B test. Do not claim
the wgpu library itself is at fault until reduced workloads reproduce the
problem. The first bgfx clear-only milestone is deliberately much smaller
than game rendering.

## Merge status

**Blocked: no bgfx game rendering implemented.** This document and
prototype are a verifiable starting point, not a finished port. The
experiment should remain a draft PR until implementation and CI prove
complete coverage.

## Actual code milestone from the latest commits

The draft now includes a Windows-only winit native display test named
`bgfx-window`, real `omsi-o3d::parse_o3d` mesh decoding, CPU bounds and
index validation, 16/32-bit index packing, bgfx vertex/index buffer upload,
`GpuMesh::draw` with source material runs, shaderc-compatible normal-colour
vertex/fragment shaders, and camera orbit/zoom controls. These are
**independent** of the shipping openOMSI renderer. A script to compile
shader binaries and runnable Windows commands are documented under
`experiments/bgfx-port/RUN_VIEWER.md`.

The native code built and eight CPU unit tests passed on both Windows and
Linux in GitHub Actions. There is still **no real-game BGFX backend**, no
GPU screenshot verification, no map streaming integration, no OMSI driving
input/physics integration, and no genuine BGFX performance measurement.
Do not call the BGFX port playable or merge-ready.

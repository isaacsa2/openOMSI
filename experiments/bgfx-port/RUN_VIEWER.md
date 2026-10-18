# Experimental native BGFX mesh viewer

This is a separate GPU test application, **NOT** the complete playable openOMSI game.
The first native mesh milestone can parse authentic OMSI .o3d content via
omsi-o3d, pack its geometry without dropping authored material runs, upload
real vertex/index buffers through bgfx, and submit them in a native Win32
window. Its only shader is a geometry-normal colour viewer: **textures,
lighting, glass, shadows, maps, driving, UI, VR and animation are missing**.

The source and all eight CPU-only tests compiled on both Windows and Linux
in the dedicated CI workflow. GPU display and compiled shader rendering are
not yet verified by CI (hosted runners are not real graphics test rigs).

## Shader setup (Windows)

Use a compatible BGFX shaderc compiler executable and BGFX source checkout
containing src/bgfx_shader.sh. Supply full paths for these two locations.

~~~powershell
& ./experiments/bgfx-port/build-shaders.ps1 -Shaderc "C:/tools/bgfx/shadercRelease.exe" -BgfxSource "C:/tools/bgfx/bgfx" -Backend dx11
~~~

The script outputs shader binaries into experiments/bgfx-port/compiled/dx11;
these files are intentionally gitignored. To test vulkan, dx12 or gl, change
the Backend parameter and compile again. Always pass binaries compiled for
the selected renderer. Profiles for Vulkan, DX12, and OpenGL need validation
on devices using those drivers.

## View a real .o3d mesh

Give the executable an actual existing path from a **licensed OMSI 2
installation**; no original content may be committed to the repo.

~~~powershell
cargo run --manifest-path experiments/bgfx-port/Cargo.toml --bin bgfx-window -- --backend dx11 --mesh "C:/Program Files (x86)/Steam/steamapps/common/OMSI 2/Vehicles/YOUR_BUS/model/YOUR_MESH.o3d" --vs experiments/bgfx-port/compiled/dx11/vs_mesh.bin --fs experiments/bgfx-port/compiled/dx11/fs_mesh.bin
~~~

YOUR_BUS/YOUR_MESH are placeholders, not file names that are known to exist.

Camera: left/right arrow orbits, up/down zooms, Escape exits. Without the
--mesh argument the window simply clears the back buffer; this tests native
initialization but not any geometry.

## This is not a performance A/B test

A single untextured mesh does not exercise the heavy scenes, textures,
pass count, shader compilation, mirrors or simulation overhead of openOMSI.
Do not compare its FPS with the full game. For full performance work use
matching maps, bus, camera, time, traffic, and GPU timings.

## Remaining integration for a playable openOMSI

- Connect native BGFX rendering to the openOMSI App/frame staging and bus
  simulation; use the existing input, save, script and multiplayer systems.
- Render streamed world tiles and roads/splines, scenery props, vehicles,
  passengers, glass/alpha-depth surfaces, night lights and shadows.
- Rebuild all wgpu WGSL pipelines in BGFX shaderc format and validate
  screenshots; there is no direct WGSL replacement.
- Implement texture streaming and GPU budgets, render-to-texture, mirrors,
  cockpit/UI/launcher panels, capture, VR/XR and device recovery.
- Prove support across supported Windows, Linux, macOS, Android, iOS and
  hardware/API combinations, and avoid regression to existing content.
- Build and run the full game using BGFX; until that succeeds the production
  wgpu renderer must remain untouched.

PR #271 MUST REMAIN DRAFT until driving and content parity work in BGFX.

## Display terrain geometry from an original map

With the same vertex/fragment shader binaries compiled above, pass a genuine
terrain height-field belonging to an installed map. This path reuses the
actual openOMSI Terrain::load parser, build_terrain_mesh generator, and the
source-preserving MeshData-to-BGFX buffer adapter.

~~~powershell
cargo run --manifest-path experiments/bgfx-port/Cargo.toml --bin bgfx-window -- --backend dx11 --terrain "C:/Program Files (x86)/Steam/steamapps/common/OMSI 2/maps/Grundorf/tile_0_0.map.terrain" --vs experiments/bgfx-port/compiled/dx11/vs_mesh.bin --fs experiments/bgfx-port/compiled/dx11/fs_mesh.bin
~~~

The terrain path above is **illustrative**: replace it with an existing
*.map.terrain file in the actual installed map. No map or model files are
bundled into this repository. The viewer shows a single height-field,
not roads, base/paint textures, scenery, scripted traffic or a drivable map.

## Optional drive-input experiment (still **not** the full openOMSI game)

The Windows viewer can now accept a real `.bus` vehicle definition
alongside a **single** O3D body part and a **small terrain-only map region**.
It reuses `omsi_vehicle::Vehicle::load` and
`omsi_sim::VehiclePhysics::from_definition`; the same openOMSI CPU
physics code advances the body pose over sampled heightfield terrain.

```powershell
cargo run --manifest-path experiments/bgfx-port/Cargo.toml --bin bgfx-window -- `
  --backend dx11 `
  --map "D:/Games/OMSI 2/maps/Grundorf/global.cfg" `
  --mesh "D:/Games/OMSI 2/Vehicles/YOUR_BUS/model/YOUR_BODY.o3d" `
  --bus-definition "D:/Games/OMSI 2/Vehicles/YOUR_BUS/YOUR_BUS.bus" `
  --vs experiments/bgfx-port/compiled/dx11/vs_mesh.bin `
  --fs experiments/bgfx-port/compiled/dx11/fs_mesh.bin
```

Paths are placeholders except for the standard `maps/Grundorf/global.cfg`
layout; replace them with files from an actual licensed installation.
**W** accelerates; **S** brakes; **A/D** steer. A follow camera shows
the model moving over generated terrain. The current preview uses
provisional torque/brake magnitudes, not the bus's engine/gearbox
script outputs, and does not implement a route, traffic, wheel collision,
passenger interaction, vehicle dashboards, synchronized animation or
correct ride height. No native GPU screenshot of the driven bus has
yet been validated. This is an **isolated mechanics integration test**
and must never be distributed as a playable openOMSI release.

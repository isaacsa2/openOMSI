# openOMSI temporal reconstruction / FidelityFX FSR 2 WGSL

## Available graphics modes

- **Standard:** Catmull-Rom spatial reconstruction, no temporal history.
- **TAA (experimental):** standalone temporal antialiasing.
- **AMD FSR 1.0:** existing WGSL EASU/RCAS spatial upscaler.
- **AMD FSR 2.0 (WGSL, experimental):** true FSR 2 algorithm ported to Rust + WGSL; no native Vulkan/DX12 DLL. The FSR 3 prototype was removed.

Only one temporal algorithm may be active. The 3D view uses projection
jitter; the interface and mirrors remain unjittered. The renderer provides
depth, motion vectors and current-frame colour, plus a history reset on
camera discontinuities. The SDK's compute output is presented at native
resolution before HUD overlays. The original FSR 1.0 path remains intact.

## Implementation

The FSR 2 WGSL port uses `sp-fidelity 0.2.0` (Rust CPU FSR2 SDK host logic,
ported AMD WGSL shader passes) and a MIT-licensed adapted wgpu backend from
`stevepryde/sp-fidelity`. Its license is preserved in
`crates/omsi-render/src/temporal/fsr2_port/LICENSE.txt`.

The renderer is built with the `fsr2-wgsl` Cargo feature by default, so
selecting FSR 2 in the game's normal graphics menu does **not** fail merely
because the executable was built without an optional shader backend.
The shader code is invoked only when the user selects the FSR 2 mode.

The `wgpu 29` port is adapted for the openOMSI custom wgpu fork; a second
`wgpu 30` device is not loaded. Core resources, motion and pipeline history
are shared with the renderer. `fsr2_input.wgsl` converts the 3D colour to
`Rgba16Float` for the compute passes. This path never performs CPU texture
readback for scaling. The separate `fsr2_sharpness` slider controls
FidelityFX RCAS; the FSR 1 slider remains independent.

## GPU compatibility and failure behavior

FSR 2 is not restricted by GPU brand. Its exact shader port nonetheless needs
`SUBGROUP`, `TEXTURE_ATOMIC`,
`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`,
`TEXTURE_FORMAT_16BIT_NORM`, and `FLOAT32_FILTERABLE` from the adapter.
These are checked explicitly rather than inferred from Vulkan, DX12, or
OpenGL names. Some older drivers, GPUs, mobile devices, and ANGLE do not
expose them. A missing feature is **not** fixed by disabling wgpu validation.

The conservative Intel Vulkan driver profile remains protected because some
drivers crash while creating the device. The other paths use the adapter's
real advertised feature set; incompatible paths should fall back to Standard
rather than crashing. Runtime log messages differentiate a missing feature,
failed pipeline, missing motion vectors and failed SDK dispatch.

## Building and testing

Run `scripts/build-fidelityfx-windows.bat` from a checkout on Windows,
or `cargo check --locked -p omsi-render --lib` on Linux. As FSR 2 is part
of the default build, a normal `cargo build --release -p omsi-app` includes
its source. On a physical GPU, test:

1. Launch with the FSR 2 mode selected; confirm a successful SDK dispatch log.
2. Compare image output against Standard/FSR1 and inspect history ghosting.
3. Test camera teleports, animated wheels, glass/rain, mirrors, resizes and
   dynamic resolution, and switching between graphics presets.
4. Compare p50/p95/p99 frametimes and VRAM consumption.

**Status:** Only successful CI compilation and GPU runtime tests establish
functional compatibility. A configured menu option or a successful pipeline
creation alone does not prove a correct FSR 2 output.

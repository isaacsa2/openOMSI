# NVIDIA DLSS Super Resolution / DLAA (integration work in progress)

Status: **experimental adapter and standalone motion-vector generation — NOT playable DLSS**.

This branch is dedicated to DLSS Super Resolution and DLAA only. It does not implement DLSS Frame Generation, NVIDIA Reflex or neural rendering. No NVIDIA DLLs, model weights, proprietary headers or binaries are included in this repository.

## Provenance and licensing

- The initial Rust Streamline integration was adapted from [Criisko/openOMSI PR #1](https://github.com/Criisko/openOMSI/pull/1), merged 2026-10-03, while the public source repository carried the MIT license.
- Original project MIT copyright notice: Copyright (c) 2026 usonskyyyy; see `NVIDIA_DLSS_THIRD_PARTY_NOTICES.txt` next to this document.
- Streamline C declarations transcribed in the public PR retain their original NVIDIA MIT notice inside `crates/omsi-render/src/nvidia_dlss.rs`.
- openOMSI's present source is AGPL-3.0-only. MIT-compatible source code may be integrated while preserving its notices, but that does **not** mean NVIDIA's proprietary DLSS runtime is AGPL-licensed or automatically redistributable. Review the current NVIDIA SDK/binary terms before packaging or enabling it in a distributed release.
- Do not use the private OmniLine 0.4.6 binaries as a source of code. Only the public PR and official Streamline documentation are implementation references.

## What is in the branch

- `nvidia_dlss.rs` provides portable mode parsing, fallback render-size calculations, and accompanying unit tests. These compile in normal builds without NVIDIA dependencies.
- `temporal_motion.rs` maintains camera/model history keyed by (scene ID, instance draw order, material slot), preserving identity across GPU entry reordering and resetting after teleports, turns, FOV/resolution changes and new maps.
- `temporal_motion.wgsl` reproduces the PR's camera + moving-model vectors and sky reprojection with the current `vs_depth`, material cutout and transmap logic.
- `temporal_gpu.rs` generates a separate RG32Float motion texture and depth attachment using the current modular batches. It runs ONLY on Windows with the experimental feature and an explicit diagnostic flag, and does not alter any player's picture.
- The application now accepts and round-trips `dlss=off|dlaa|quality|balanced|performance|ultra_performance` in `settings.cfg`. The value is intentionally **not yet wired to the renderer** and has no effect on the image. This preserves existing graphics settings while the integration is developed.
- `.github/workflows/dlss-adapter.yml` compiles the Windows-only native feature and exercises portable mode tests on Linux; failures from those jobs are treated as blockers, not ignored.
- The Windows/D3D12 Streamline adapter from the MIT PR is kept behind the **off-by-default** Cargo feature `nvidia-dlss-experimental`. It has **not** yet been integrated with the current renderer and **must not** be described as working DLSS.
- The runtime, when eventually connected, is intended to load NVIDIA libraries explicitly from a user-supplied location and to fail cleanly when unsupported.

## Remaining work before a playable implementation

1. **Motion-vector milestone implemented for diagnostic verification:** evaluate the RG32Float output in a compatible GPU, compare moving buses and a rotating camera, and test how tree sway and transparent surfaces behave. The jitter Halton calculation is implemented, but still needs to be applied to the actual camera projection when DLSS is running.
2. Integrate the diagnostic motion/depth textures with the DLSS Streamline evaluation and D3D12 command submission *after* the 3D scene but *before* HUD overlays; verify wgpu queue ordering, texture states, lifetime, command allocator reuse and GPU fences. Audit all unsafe sections.
3. Add a separate DLSS output texture and an sRGB-correct final pass; do not apply standard upscaling, FSR, FXAA, or MSAA concurrently with DLSS.
4. Wire settings and launcher/game menus **only when the native integration is functioning**, with the default `off`, RTX/DX12 capability detection, and localised help/diagnostics.
5. Ensure fallback reinstates the previous complete graphics path (MSAA, FXAA, scale and any required pipeline state), even when a Windows/NVIDIA device fails to load the DLLs.
6. Verify regular Windows, Linux, macOS and Android builds, shader validation on GL/Vulkan, and GPU runtime tests on compatible RTX hardware. Benchmark frametime and check ghosting, alpha cutouts, transparent objects, moving vehicles, glass, mirrors, VR and device loss.

## Test guidance

Portable tests (once CI is available):

```sh
cargo test -p omsi-render --lib nvidia_dlss
```

Windows compile check for the experimental native adapter and motion pass (not a runtime test):

```sh
cargo check -p omsi-render --features nvidia-dlss-experimental --target x86_64-pc-windows-msvc
```

For a Windows/DX12 **diagnostic run only** (build with `nvidia-dlss-experimental`), set `OMSI_DLSS_MOTION_DEBUG=1` before launching the game. It generates motion/depth targets in the main desktop view without changing the final image. Do not set this for ordinary gameplay.

A successful build does **not** prove Streamline initialization, DLSS image quality or NVIDIA licensing compliance.

CI jobs: `DLSS experimental adapter checks` (Windows native feature and Linux portable tests) and the standard `Release` matrix. Keep this PR in Draft until both compile cleanly and RTX hardware validation is documented. Do not merge this draft until the missing renderer integration and real-GPU evidence are supplied.

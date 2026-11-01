# NVIDIA DLSS Super Resolution / DLAA (integration work in progress)

Status: **opt-in Windows/DX12 beta implementation present — native RTX runtime and image quality NOT verified yet**.

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
- `dlss_beta.rs` now provides the opt-in experimental desktop SR/DLAA presentation path: select `OMSI_DLSS_BETA=quality|balanced|performance|ultra_performance|dlaa`, load the user-provided **official** Streamline runtime, render the 3D scene at Streamline's requested size, evaluate native D3D12 and compose the full-resolution output with the unscaled HUD. This path uses a temporal projection jitter; the motion and camera matrices remain unjittered.
- `dlss_present.wgsl` presents NVIDIA's full-size output without running it through the ordinary spatial upscaler. When initialization or evaluation fails, the game renders the ordinary picture; the experimental path remains off for that session.
- The **single standard Windows x64 release package** is built by `scripts/build-windows.cmd` and includes the opt-in DLSS feature in the normal `openomsi.exe`. No separate DLSS beta executable/artifact is generated, and NVIDIA proprietary DLLs are never bundled. See `docs/NVIDIA_DLSS_BETA_TEST.md`.

- The application and launcher parse, write and round-trip `dlss=off|dlaa|quality|balanced|performance|ultra_performance` in `settings.cfg`. The beta renderer now reads that value at startup **only** when Windows DirectX 12 is explicitly selected, and always rechecks the actual active device. Changing it in the in-game menu requires restarting the game.
- `.github/workflows/dlss-adapter.yml` compiles the Windows-only native feature and exercises portable mode tests on Linux; failures from those jobs are treated as blockers, not ignored.
- The Windows/D3D12 Streamline adapter is behind Cargo feature `nvidia-dlss-experimental`: the **normal Windows x64 release enables that feature at build time**, while the user-facing DLSS mode is `off` by default. The native runtime is loaded only for compatible DX12 opt-in configurations; RTX functionality still needs hardware validation.
- The runtime, when eventually connected, is intended to load NVIDIA libraries explicitly from a user-supplied location and to fail cleanly when unsupported.

## Remaining work before a playable implementation

1. Motion vectors and Halton jitter have been integrated into the opt-in beta frame; **verify them on a real RTX GPU** (moving buses, camera pans, trees, alpha-cutouts, rain and reflections). No real-GPU execution has been proven.
2. The opt-in beta has Streamline evaluation and HUD composition in source; **validate** wgpu/D3D12 queue ordering, texture state restoration, lifetime, command allocator reuse and GPU fences. Audit all unsafe sections before upstream.
3. Add a separate DLSS output texture and an sRGB-correct final pass; do not apply standard upscaling, FSR, FXAA, or MSAA concurrently with DLSS.
4. The beta selector is present in the launcher and in-game graphics menu, gated by DirectX 12 (and the actual renderer for the in-game menu). Validate RTX hardware behaviour before a general release. Settings changes need a restart in this beta.
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

For a Windows/DX12 **diagnostic run only** (build with `nvidia-dlss-experimental`), set `OMSI_DLSS_MOTION_DEBUG=1` before launching the game. It generates motion/depth targets in the main desktop view without changing the final image. To test native DLSS output instead, see the dedicated beta guide.

A successful build does **not** prove Streamline initialization, DLSS image quality or NVIDIA licensing compliance.

CI jobs: `DLSS experimental adapter checks` (Windows native feature and Linux portable tests) and the standard `Release` matrix. Keep this PR in Draft until both compile cleanly and RTX hardware validation is documented. Do not merge this draft until the missing renderer integration and real-GPU evidence are supplied.

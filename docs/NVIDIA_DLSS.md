# NVIDIA DLSS Super Resolution / DLAA (integration work in progress)

Status: **opt-in Windows/DX12 beta implementation present — native RTX runtime and image quality NOT verified yet**.

This branch is dedicated to DLSS Super Resolution and DLAA only. It does not implement DLSS Frame Generation, NVIDIA Reflex or neural rendering. No NVIDIA DLLs or model weights are committed to the source repository. The Windows x64 release packaging retrieves the signed production runtime from NVIDIA's official SDK and includes its license notices in the packaged ZIP.

## Provenance and licensing

- The initial Rust Streamline integration was adapted from [Criisko/openOMSI PR #1](https://github.com/Criisko/openOMSI/pull/1), merged 2026-10-03, while the public source repository carried the MIT license.
- Original project MIT copyright notice: Copyright (c) 2026 usonskyyyy; see `NVIDIA_DLSS_THIRD_PARTY_NOTICES.txt` next to this document.
- Streamline C declarations transcribed in the public PR retain their original NVIDIA MIT notice inside `crates/omsi-render/src/nvidia_dlss.rs`.
- openOMSI's source is AGPL-3.0-only. The NVIDIA Streamline SDK's distribution instructions explicitly list the four modules to ship, but they retain separate NVIDIA licenses, including NGX. License compatibility with AGPL and the distribution requirements must be reviewed before an official public release.
- Do not use the private OmniLine 0.4.6 binaries as a source of code. Only the public PR and official Streamline documentation are implementation references.

## What is in the branch

- `nvidia_dlss.rs` provides portable mode parsing, fallback render-size calculations, and accompanying unit tests. These compile in normal builds without NVIDIA dependencies.
- `temporal_motion.rs` maintains camera/model history keyed by (scene ID, instance draw order, material slot), preserving identity across GPU entry reordering and resetting after teleports, turns, FOV/resolution changes and new maps.
- `temporal_motion.wgsl` reproduces the PR's camera + moving-model vectors and sky reprojection with the current `vs_depth`, material cutout and transmap logic.
- `temporal_gpu.rs` generates a separate RG32Float motion texture and depth attachment using the current modular batches. It runs ONLY on Windows with the experimental feature and an explicit diagnostic flag, and does not alter any player's picture.
- `dlss_beta.rs` provides the optional desktop SR/DLAA path: select a mode in the launcher/game, load the **official bundled** production Streamline runtime (or use `OMSI_STREAMLINE_DIR` override), render the scene at its requested resolution and present the full-size output before the HUD. The projection uses temporal jitter while the motion matrices remain unjittered.
- `dlss_present.wgsl` presents NVIDIA's full-size output without running it through the ordinary spatial upscaler. When initialization or evaluation fails, the game renders the ordinary picture; the experimental path remains off for that session.
- The **single standard Windows x64 release package** is built by `scripts/build-windows.cmd` and includes opt-in DLSS in the normal `openomsi.exe`. The Release workflow downloads the official Streamline 2.14.1 SDK ZIP, verifies SHA-256 and NVIDIA Authenticode signatures, and packages only `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll` and `nvngx_dlss.dll` plus NVIDIA's license notices. No DLSS-G or Reflex, and no second executable. See `scripts/windows/package-streamline.ps1` and `docs/NVIDIA_DLSS_BETA_TEST.md`.

- The application and launcher parse, write and round-trip `dlss=off|dlaa|quality|balanced|performance|ultra_performance` in `settings.cfg`. The beta renderer now reads that value at startup **only** when Windows DirectX 12 is explicitly selected, and always rechecks the actual active device. Changing it in the in-game menu requires restarting the game.
- `.github/workflows/dlss-adapter.yml` compiles the Windows-only native feature and exercises portable mode tests on Linux; failures from those jobs are treated as blockers, not ignored.
- The Windows/D3D12 Streamline adapter is behind Cargo feature `nvidia-dlss-experimental`: the **normal Windows x64 release enables that feature at build time**, while the user-facing DLSS mode is `off` by default. The native runtime is loaded only for compatible DX12 opt-in configurations; RTX functionality still needs hardware validation.
- The runtime loads the NVIDIA DLLs from beside the game by default, or from a user-specified `OMSI_STREAMLINE_DIR`; initialization failure must keep the conventional renderer active.

## Remaining work before a playable implementation

1. An RTX beta-test log on 2026-11-01 confirmed Streamline 2.14.1 initialization and DLSS Balanced sizing (1114×626 input → 1920×1080 output); wgpu then reported the motion-sky pipeline missing its camera bind group at index 0. The binding has been moved before the sky draw. **Retest the correction on the same RTX system** before claiming a working frame, especially in Enhanced+.
2. The opt-in beta has Streamline evaluation and HUD composition in source; **validate** wgpu/D3D12 queue ordering, texture state restoration, lifetime, command allocator reuse and GPU fences. Audit all unsafe sections before upstream.
3. Verify the beta's separate DLSS output texture, HDR/sRGB handling and final pass. Test that standard upscaling, FSR, FXAA and MSAA are not applied concurrently with DLSS.
4. The beta selector is only shown in the launcher when Windows DXGI discovers an NVIDIA RTX candidate and DX12 is selected. In-game it additionally requires that **the actual wgpu renderer** uses an RTX adapter on DX12. The NVIDIA Streamline capability query and official runtime still decide actual support at activation. Changes need a restart.
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

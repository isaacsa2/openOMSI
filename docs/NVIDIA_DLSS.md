# NVIDIA DLSS Super Resolution / DLAA (integration work in progress)

Status: **experimental adapter staging — not connected to the rendering pipeline**.

This branch is dedicated to DLSS Super Resolution and DLAA only. It does not implement DLSS Frame Generation, NVIDIA Reflex or neural rendering. No NVIDIA DLLs, model weights, proprietary headers or binaries are included in this repository.

## Provenance and licensing

- The initial Rust Streamline integration was adapted from [Criisko/openOMSI PR #1](https://github.com/Criisko/openOMSI/pull/1), merged 2026-10-03, while the public source repository carried the MIT license.
- Original project MIT copyright notice: Copyright (c) 2026 usonskyyyy; see `NVIDIA_DLSS_THIRD_PARTY_NOTICES.txt` next to this document.
- Streamline C declarations transcribed in the public PR retain their original NVIDIA MIT notice inside `crates/omsi-render/src/nvidia_dlss.rs`.
- openOMSI's present source is AGPL-3.0-only. MIT-compatible source code may be integrated while preserving its notices, but that does **not** mean NVIDIA's proprietary DLSS runtime is AGPL-licensed or automatically redistributable. Review the current NVIDIA SDK/binary terms before packaging or enabling it in a distributed release.
- Do not use the private OmniLine 0.4.6 binaries as a source of code. Only the public PR and official Streamline documentation are implementation references.

## What is in the branch

- `nvidia_dlss.rs` provides portable mode parsing, fallback render-size calculations, and accompanying unit tests. These compile in normal builds without NVIDIA dependencies.
- The Windows/D3D12 Streamline adapter from the MIT PR is kept behind the **off-by-default** Cargo feature `nvidia-dlss-experimental`. It has **not** yet been integrated with the current renderer and **must not** be described as working DLSS.
- The runtime, when eventually connected, is intended to load NVIDIA libraries explicitly from a user-supplied location and to fail cleanly when unsupported.

## Remaining work before a playable implementation

1. Port the depth/motion-vector pass and temporal jitter to the modular 2026-11-01 renderer, including scene-object previous-frame identity and reset rules on teleports, re-uploads and resolution changes.
2. Integrate D3D12 command submission *after* the 3D scene but *before* HUD overlays; verify wgpu queue ordering, texture states, lifetime, command allocator reuse and GPU fences. Audit all unsafe sections.
3. Add a separate DLSS output texture and an sRGB-correct final pass; do not apply standard upscaling, FSR, FXAA, or MSAA concurrently with DLSS.
4. Wire settings and launcher/game menus **only when the native integration is functioning**, with the default `off`, RTX/DX12 capability detection, and localised help/diagnostics.
5. Ensure fallback reinstates the previous complete graphics path (MSAA, FXAA, scale and any required pipeline state), even when a Windows/NVIDIA device fails to load the DLLs.
6. Verify regular Windows, Linux, macOS and Android builds, shader validation on GL/Vulkan, and GPU runtime tests on compatible RTX hardware. Benchmark frametime and check ghosting, alpha cutouts, transparent objects, moving vehicles, glass, mirrors, VR and device loss.

## Test guidance

Portable tests (once CI is available):

```sh
cargo test -p omsi-render --lib nvidia_dlss
```

Windows compile check for the experimental native adapter (not a runtime test):

```sh
cargo check -p omsi-render --features nvidia-dlss-experimental --target x86_64-pc-windows-msvc
```

A successful build does **not** prove Streamline initialization, DLSS image quality or NVIDIA licensing compliance. Do not merge this draft until the missing renderer integration and real-GPU evidence are supplied.

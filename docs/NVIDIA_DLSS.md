# NVIDIA DLSS and AMD FSR 3

Temporal upscaling (`crates/omsi-render/src/temporal`, from openOMSI-org/openOMSI#3681): the
picture is drawn smaller and brought up to the window with the last frames' help. Settings →
Graphics → Upscaling, `upscaler=dlss` or `fsr3` and `upscale_quality`, from the next start.

- **DLSS** is offered only where Windows lists an NVIDIA RTX adapter
  (`omsi_cfg::upscaler_files::rtx_present`); NVIDIA's runtime still checks the card and its
  driver when it starts. **FSR 3** is offered on any card.
- **DirectX 12 or Vulkan.** With Vulkan chosen (Settings → Graphics API) and the upscaler's
  Vulkan DLLs present, the game stays on Vulkan; otherwise it starts on DirectX 12.
  - FSR 3 on Vulkan: `amd_fidelityfx_loader_vk.dll`, `amd_fidelityfx_upscaler_vk.dll`.
  - DLSS on Vulkan: Streamline is started before the device and its device extensions added to
    wgpu's. Where DLSS asks for Vulkan features or queues wgpu's device is not made with, or the
    adapter lacks an extension, DLSS stays off and `game.log` says why (choose DirectX 12).
- Anything that fails is logged once and the picture is drawn without the upscaler.

## Packaging

The Windows x64 release carries the vendors' signed DLLs (`.github/workflows/release.yml`):
NVIDIA Streamline 2.14.1 (SDK ZIP SHA-256 pinned, `scripts/windows/package-streamline.ps1`,
notices in `NVIDIA_DLSS_THIRD_PARTY_NOTICES.txt`) and AMD FidelityFX SDK 2.4.0 at a pinned
commit (MIT, `LICENSE-FidelityFX.md`). None of them are in the repository. Whether NVIDIA's
licences (Streamline, NGX) suit an AGPL project's release is to be reviewed before upstream
ships them.

## Not verified yet

Nothing here has run on a GPU: DLSS and FSR 3 on DirectX 12 and Vulkan need testing on an RTX
card (and FSR 3 on an AMD or Intel one) before they are called working.

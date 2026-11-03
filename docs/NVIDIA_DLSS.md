# NVIDIA DLSS and AMD FSR 3

Temporal upscaling (`crates/omsi-render/src/temporal`, based on openOMSI-org/openOMSI#3681)
is opt-in: Settings → Graphics → Upscaling, `upscaler=dlss` or `fsr3` plus
`upscale_quality`. A change takes effect on the next start.

- **DLSS** is offered only with an NVIDIA RTX adapter on Windows, subject to the
  Streamline runtime's compatibility checks.
- **FSR 3** can be offered on other vendors' GPUs, subject to runtime support.
- **DirectX 12 or Vulkan** is selected according to the graphics setting and
  the relevant DLLs; failure disables the upscaler and is logged in `game.log`.
- The source integration does not contain the vendor's production DLLs.

## Windows x64 packaging: redistribution on hold by default

`.github/workflows/release.yml` includes *optional* vendor packaging steps,
but does **not** bundle either vendor's DLLs by default. The maintainer must
independently complete a license review and explicitly set a GitHub Actions
repository/organization variable to the literal string `true`:

- `OPENOMSI_NVIDIA_REDISTRIBUTION_APPROVED` for NVIDIA Streamline 2.14.1
  and the DLSS/NGX runtime. The script verifies the SDK ZIP SHA-256 and
  Authenticode signatures and includes the applicable notices.
- `OPENOMSI_AMD_REDISTRIBUTION_APPROVED` for the FidelityFX SDK 2.4.0
  signed DX12/Vulkan DLLs at its pinned commit, including the vendor license.

These are **maintainer-controlled approval switches**, not an assertion that
permission has been obtained. Do not set them merely to make CI pass.
A source build/release can run without those SDK DLLs; the corresponding
upscaler choices remain unavailable until the necessary libraries are present.

**Licensing is not interchangeable:** Streamline's publicly licensed
source/header portions do not establish the terms for the proprietary
`nvngx_dlss.dll`; AMD's signed binary distribution terms are not a blanket MIT
grant. Whether bundling these SDKs alongside openOMSI (AGPLv3) is permitted
requires review of the **exact** SDK binaries, license files, method of
integration, and distribution. See
[`UPSCALER_THIRD_PARTY_LICENSING.md`](UPSCALER_THIRD_PARTY_LICENSING.md).

## Not verified yet

DLSS and FSR 3 still require GPU validation on DX12 and Vulkan. Build/CI status
and runtime functionality are separate from legal redistribution approval.

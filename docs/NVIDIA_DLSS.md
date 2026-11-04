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

## Windows x64 packaging: AMD included, NVIDIA DLSS/NGX on hold

`.github/workflows/release.yml` packages the **four specific FidelityFX
SDK 2.4.0 signed DLLs** on every Windows x64 release. The SDK's pinned
`Kits/FidelityFX/docs/license.md` explicitly lists those four filenames
as **MIT-licensed exceptions** (see its `These source files ... are subject
to the following license` section). The workflow checks that the four are
listed and includes the full license and third-party notices.

NVIDIA Streamline 2.14.1's MIT-covered components and the proprietary DLSS/
NGX runtime must not be treated as one license. The workflow **does not bundle
NVIDIA DLLs by default**. `OPENOMSI_NVIDIA_REDISTRIBUTION_APPROVED=true`
is a maintainer-controlled release switch, to be set **only** after
documented compatibility/redistribution clearance for the actual SDK ZIP
and openOMSI's AGPLv3 distribution.

Shipping AMD's four listed MIT binaries does not imply that every file in
the FidelityFX SDK is MIT-licensed. The source code does not contain the
vendors' production DLLs. See
[`UPSCALER_THIRD_PARTY_LICENSING.md`](UPSCALER_THIRD_PARTY_LICENSING.md).

## Not verified yet

DLSS and FSR 3 still require GPU validation on DX12 and Vulkan. Build/CI status
and runtime functionality are separate from legal redistribution approval.

# Third-party temporal upscaler licensing — decision record

**Status (2026-11-03): AMD 2.4.0's four listed FSR upscaler DLLs CLEARED under MIT;
NVIDIA's proprietary DLSS/NGX runtime NOT CLEARED for bundled AGPLv3 distribution.**
This technical record is not legal advice or a guarantee for future SDK versions.

## Scope

- openOMSI is AGPLv3-licensed.
- NVIDIA: Streamline 2.14.1; `sl.interposer.dll`, `sl.common.dll`,
  `sl.dlss.dll` and `nvngx_dlss.dll`.
- AMD: FidelityFX SDK 2.4.0, commit
  `7539ae8b6568ad29ed78a753404351daf3b5cca7`:
  `amd_fidelityfx_loader_dx12.dll`, `amd_fidelityfx_upscaler_dx12.dll`,
  `amd_fidelityfx_loader_vk.dll`, `amd_fidelityfx_upscaler_vk.dll`.

## AMD: the four exact binary files have an MIT exception

Read the *whole* pinned `Kits/FidelityFX/docs/license.md`, not just its
opening paragraph. The first section sets a restrictive binary-only license
for files **not** excepted below. Beginning at `These source files`, however,
it lists **all four exact signedbin DLLs** above, then states those listed
files `are subject to the following license`: the MIT grant.

This **explicit exception** means these four signed DLLs can be redistributed
together with the AGPLv3 openOMSI distribution under their MIT terms, with
copyright/permission notice preserved. This conclusion is limited to those
four filenames at the pinned commit — it does not cover every binary, SDK
tool, library or future version.

The Windows x64 workflow checks the MIT exception list, copies only the four
allowlisted DLLs, includes the complete SDK license, a short mapping notice
and the SDK's third-party notices. AMD packaging is on by default.

Source:
https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/7539ae8b6568ad29ed78a753404351daf3b5cca7/Kits/FidelityFX/docs/license.md

## NVIDIA: MIT Streamline core versus proprietary NGX runtime

The Streamline repository's `license.txt` grants MIT terms to the Streamline
source/core portions. The production `nvngx_dlss.dll` is **separately
licensed** by the NVIDIA RTX/NGX SDK EULA (`nvngx_dlss.license.txt` inside
the official SDK distribution). NVIDIA's official Streamline integration guide
identifies this DLL as required for DLSS.

The NVIDIA RTX SDK license grants conditional permission to distribute
SDK material incorporated in applications (section 1(c)) but imposes specific
distribution obligations (section 2(c)) and prohibits use that would cause
the proprietary SDK to become subject to an open-source license (section
4(e)). The RTX supplement also specifies notification before **commercial**
DLSS/NGX application releases (section 4). These provisions do not
automatically prohibit every open-source game; they do mean that the
interaction with AGPLv3 needs an affirmative, case-specific determination.

The proprietary NGX DLL is *not* generally covered by the Streamline MIT
license. Because openOMSI's optional DLSS integration dynamically loads this
DLL, merely placing it beside `openomsi.exe` or calling it a separate file
does not prove a GPL/AGPL-compatible aggregate. The GNU GPL FAQ treats
linking with GPL-incompatible libraries as needing an appropriate exception
unless a defined exception applies. A custom linking exception for AGPL code
cannot be granted unilaterally for other authors' contributions.

**Action:** keep `OPENOMSI_NVIDIA_REDISTRIBUTION_APPROVED` unset and do not
bundle the DLSS/NGX runtime in public openOMSI packages pending review of the
exact SDK ZIP terms and, if necessary, written approval from NVIDIA and the
relevant openOMSI rights holders. NVIDIA's open-source Streamline components
may be distributed on their own under applicable MIT terms, but do not
provide DLSS functionality without the NGX runtime.

Official references:
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/docs/ProgrammingGuide.md
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs

## Release and test distinction

Legal ability to redistribute a DLL is separate from compatibility,
successful compilation and GPU validation. This review does not prove
the AMD or NVIDIA upscaler works on DX12/Vulkan, does not certify CI and
does not replace a review of any other SDK dependencies.

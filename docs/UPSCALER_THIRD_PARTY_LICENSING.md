# Third-party temporal upscaler licensing — decision record

**Status (2026-11-03): NOT CLEARED for automatic public redistribution.**
This is a technical licensing assessment for maintainers, **not legal advice**
or a definitive determination of compatibility. The game source remains AGPLv3;
vendor DLLs are not relicensed under the AGPL by including license notices.

## Scope

- Project: openOMSI, distributed under GNU AGPL version 3.
- NVIDIA: Streamline **2.14.1**, production `sl.interposer.dll`,
  `sl.common.dll`, `sl.dlss.dll`, `nvngx_dlss.dll`.
  Packaging source: official versioned Streamline release ZIP, with pinned
  SHA-256 and individual license notices.
- AMD: FidelityFX SDK **2.4.0** signed `amd_fidelityfx_loader_*.dll` and
  `amd_fidelityfx_upscaler_*.dll` from commit
  `7539ae8b6568ad29ed78a753404351daf3b5cca7` (DX12 and Vulkan).
- The licensing of the adapter source code is distinct from the distribution
  terms of the compiled vendor binaries.

## Findings

### NVIDIA — rights still need clearance

The NVIDIA Streamline repository's MIT-licensed source and headers **do not
automatically license the separately provided DLSS/NGX production runtime**.
The SDK ZIP supplies distinct notices, including `nvngx_dlss.license.txt`.
NVIDIA's published RTX/NGX SDK agreements contain distribution conditions
and restrictions on subjecting their SDK to open-source license obligations.
The exact terms that govern this **particular Streamline 2.14.1 ZIP and
nvngx_dlss.dll** must be compared with the proposed distribution, including
any attribution or notification requirements. Preserving the notice alone
does not establish AGPL compatibility.

NVIDIA's integration guide identifies the required runtime files for an
application, but technical packaging instructions do not override its
applicable binary license.

Official references:
- https://github.com/NVIDIA-RTX/Streamline/tree/v2.14.1
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/docs/ProgrammingGuide.md
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt

### AMD — not blanket MIT for signed binaries

The *pinned* SDK's `Kits/FidelityFX/docs/license.md` contains binary
redistribution permission subject to conditions including reproducing its
notices and restrictions on reverse engineering. This is **not the MIT
license**, even though portions of source code may carry MIT notices.
The exact applicability and compatibility of those restrictions with
AGPLv3 for openOMSI's distribution must be reviewed; the permission to
distribute a binary is not by itself permission to relicense it under AGPL.

Official reference:
- https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/7539ae8b6568ad29ed78a753404351daf3b5cca7/Kits/FidelityFX/docs/license.md

### AGPLv3 and integration

AGPLv3 imposes corresponding-source and downstream-rights obligations for
covered combined works. Loading an optional, proprietary runtime through a
DLL does **not automatically settle** whether the binaries form an aggregate,
a combined work, or qualify for an exception (such as a system library).
This turns on the actual integration and distribution facts. An additional
permission/exception cannot be issued for third-party AGPL code without
the rights holders' authority.

References:
- https://www.gnu.org/licenses/agpl-3.0.html
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs
- https://www.gnu.org/licenses/gpl-faq.html#SystemLibraryException

## Required before turning on vendor packaging

1. Retain and inspect the *exact* legal notices extracted from the versioned
   production ZIP/SDK commit, and identify which notice governs **each**
   shipped DLL (including NVIDIA NGX and AMD signed binaries).
2. Record a competent licensing review of AGPLv3 compatibility for the
   proposed integration and binary redistribution; obtain permission or
   written clarification from the applicable vendor/rightsholders if needed.
3. Verify all notice, attribution, branding, distribution and (if applicable)
   pre-release/commercial-release notification requirements. Do not assume
   NVIDIA's or AMD's permissions are identical.
4. Document the approval and only then enable the corresponding GitHub
   Actions variable: `OPENOMSI_NVIDIA_REDISTRIBUTION_APPROVED=true` and/or
   `OPENOMSI_AMD_REDISTRIBUTION_APPROVED=true`.
5. Validate the resulting Windows x64 artifact and any installer contents
   before public publication.

The release workflow deliberately **skips shipping vendor DLLs by default**,
while preserving source integration and optional build steps. This is a
temporary distribution safeguard, not a claim that the SDKs cannot lawfully
be used or that the legal questions have been resolved. It also does not
imply that separately distributing vendor binaries is automatically safe.

# NVIDIA DLL distribution and license scope

This notice applies **only** to the third-party NVIDIA runtime binaries
shipped alongside the Windows x64 build of openOMSI:

- `sl.interposer.dll`
- `sl.common.dll`
- `sl.dlss.dll`
- `nvngx_dlss.dll`

These DLLs come unmodified from NVIDIA Streamline SDK 2.14.1, downloaded
from the original NVIDIA release and checked by a pinned ZIP SHA-256 and
per-DLL Authenticode signature validation. The package also includes
`NVIDIA-Streamline-LICENSE.txt`, `nvngx_dlss.license.txt` and
`NVIDIA-Streamline-NOTICE.txt`.

## Separate vendor licensing; no relicensing

The **entire original openOMSI codebase**, including the original Rust/WGSL
DLSS/FSR integration, launcher/settings code and packaging scripts, stays
under the project's GNU AGPLv3 license. No special project-code exception
or relicensing is created by the presence of separately licensed binaries.

The four listed **NVIDIA vendor binaries** are **not** openOMSI source code
and are **not** licensed under openOMSI's AGPLv3 or its retained historical
MIT notice. Streamline's MIT-covered portions and
the separate proprietary NVIDIA DLSS/NGX runtime must retain their original
copyright notices, license terms and limitations. NVIDIA retains the
rights in its SDK and binaries; openOMSI does not grant downstream rights
beyond NVIDIA's license. Distribution does **not** imply NVIDIA sponsorship,
endorsement or an assignment of distributor liability to NVIDIA.

No part of the **openOMSI-authored integration code** is removed from
AGPLv3 by this notice. Third-party code originating from NVIDIA's MIT
headers retains its own attribution and license as applicable. The vendor
runtime and the project source have different licensors; neither license
is applied to the other's software by this notice.

## Important legal limitation

The NVIDIA RTX SDK/NGX license conditionally permits application-integrated
distribution but requires protection of NVIDIA's rights and prohibits
putting its SDK under an open-source license. Labelling DLLs as separate,
or loading them at runtime, does **not automatically prove** that a combined
AGPLv3 application and the proprietary runtime can be legally conveyed.
This classification of vendor binaries does not by itself resolve every
question about combined AGPL/proprietary distribution; that assessment is
separate from keeping each component under its correct license.

The packaging workflow currently bundles these DLLs at the maintainer's
direction. This record is documentation of scope and provenance, **not
a completed legal clearance**. Before a public release, comply with the applicable vendor terms and
resolve any outstanding rights or compatibility questions.

Official vendor references:
- https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.1
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html

GNU references:
- https://www.gnu.org/licenses/agpl-3.0.html#section7
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs

No additional permissions or exceptions for openOMSI source code are granted
by this notice. All project-authored source remains AGPLv3.
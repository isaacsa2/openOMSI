# Proposed AGPLv3 section 7 additional permission: NVIDIA Streamline / DLSS ONLY

**Status:** narrowly scoped permission text for review in PR #371. This file
must **not** be represented as an effective exception for all of openOMSI,
or as NVIDIA's approval, until the necessary copyright holders have expressly
adopted it for their contributions and the vendor license requirements are met.

## Additional permission for expressly authorized AGPL-covered code

For the copyrightable openOMSI source-code portions whose copyright holder(s)
have **expressly adopted this notice**, those holders grant an additional
permission under section 7 of the GNU Affero General Public License,
version 3 only, to link or otherwise combine those authorized portions with
the **unmodified, NVIDIA-distributed Streamline SDK 2.14.1 DLSS runtime**
and to convey the resulting combination, subject to independently satisfying
all applicable NVIDIA licenses and other third-party rights.

The NVIDIA components covered by this **specific** additional permission are
limited to the official x64 production files:

- `sl.interposer.dll`
- `sl.common.dll`
- `sl.dlss.dll`
- `nvngx_dlss.dll`

The purpose of this permission is interoperability with **NVIDIA DLSS Super
Resolution / DLAA via Streamline**, not a blanket proprietary-SDK exception.
It does not apply to other NVIDIA SDKs, other proprietary software, arbitrary
versions or patched DLLs, or any other vendor's binaries.

## Rights and limits

1. **openOMSI remains AGPL-3.0-only.** All AGPLv3 source disclosure, notices
   and other obligations continue to apply to the AGPL-covered code, except
   only to the extent of a valid additional permission above.
2. **NVIDIA retains its own rights.** This notice does not license,
   sublicense, relicense, or designate NVIDIA DLLs as AGPL-covered source. The
   proprietary NVIDIA NGX runtime remains under NVIDIA's terms. NVIDIA has
   not approved, sponsored or endorsed the project through this notice.
3. **Only a rights holder may grant this permission for their code.** Nothing
   here extends to contributions for which the necessary owners have not
   consented. Coauthorship of a PR or being a project maintainer does not
   establish the right to waive the terms of other contributors' work.
4. **This is not a substitute for NVIDIA clearance.** NVIDIA's RTX/NGX terms,
   including the open-source-license limitation and distribution obligations,
   must be met independently. Packaging or runtime dynamic loading alone
   does not settle that question.

## Adoption record and release requirement

- **isaacsa2 / Isaac S.** has directed the proposal of this permission for
  his **own copyrightable contributions** to the DLSS integration; no
  authorization for other authors' code is asserted by that direction.
- **m3rlinek01**, the author of #3681's initial upscaling work: direct,
  written opt-in for the specific affected code is **still to be recorded**
  in the PR or an equivalent durable record.
- **Other holders of affected pre-existing AGPL code:** determine the extent
  of their involvement and document any additional consents needed.

This document is a **limited-permission proposal with incomplete adoption**;
it is not evidence that a public combined AGPLv3/NVIDIA proprietary release
has been legally cleared. When permissions are obtained, identify the exact
covered files/portions and corresponding consenting holders, and add the
relevant notice to those files before claiming the exception applies to them.

Implementation scope: [DLSS_AUTHORSHIP_SCOPE.md](DLSS_AUTHORSHIP_SCOPE.md).
Vendor-specific details: [NVIDIA_DLL_LICENSE_SCOPE.md](NVIDIA_DLL_LICENSE_SCOPE.md).

References:
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs
- https://www.gnu.org/licenses/agpl-3.0.html#section7
- https://github.com/NVIDIA-RTX/Streamline/blob/v2.14.1/license.txt
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt
- https://docs.nvidia.com/ngx/latest/ngx-eula/index.html

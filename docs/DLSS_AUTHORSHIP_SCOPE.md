# DLSS/FSR integration: code and rights boundaries

Reference: [openOMSI-org/openOMSI#3681](https://github.com/openOMSI-org/openOMSI/pull/3681).
This inventory separates implementation provenance from license ownership. It is not a
copyright assignment or a line-by-line authorship determination.

## Integration developed for #3681

| Area | Representative paths | Attribution available |
| --- | --- | --- |
| Temporal rendering, motion vectors, DX12 and the initial vendor adapters | `crates/omsi-render/src/temporal/{frame,mod,d3d12,streamline,fidelityfx}.rs`, `crates/omsi-render/src/temporal/motion.wgsl`, `crates/omsi-render/src/passes/temporal.rs`, `crates/omsi-render/src/pipelines/temporal.rs` | PR #3681 describes this work as originating with **m3rlinek01**. Actual per-line authorship and later modifications require commit/blame review. |
| Vulkan integration and vendor device capabilities | `crates/omsi-render/src/temporal/vulkan.rs`, `crates/omsi-render/src/gpu/upscaler_device.rs` and related changes | PR #3681 attributes Vulkan integration to **isaacsa2**; files have also been integrated and revised through the shared branch. |
| RTX detection, runtime availability and packaging | `crates/omsi-cfg/src/upscaler_files.rs`, `scripts/windows/package-streamline.ps1`, `.github/workflows/release.yml` | PR #3681 attributes RTX gating and Windows vendor packaging to **isaacsa2**. |
| Licensing notes and integration tests | `docs/NVIDIA_*.md`, `docs/UPSCALER_THIRD_PARTY_LICENSING.md`, `docs/UPSCALING_TESTS.md`, tests in `omsi-cfg`, `omsi-render`, and `omsi-app` | PR #3681 credits **isaacsa2** for test runs and license/packaging documentation; individual changes may be shared. |

The paths above are an engineering map, **not** a declaration that the named person
exclusively owns every line in the file. In particular, the Streamline and FidelityFX
bindings reproduce or adapt third-party interface definitions with their own notices.

## Existing AGPLv3 code touched by the integration

The upscaler also changes existing code in `crates/omsi-render/src/{render.rs,lib.rs,passes/,scene/,gpu/}`,
`crates/omsi-app/src/`, `crates/omsi-cfg/src/` and launcher/UI files.
Those changes form part of openOMSI's pre-existing AGPLv3 application. A new file
or a separate DLL is not proof that an entire combined application is free of
other contributors' rights.

## Third-party components are separate from project authorship

- NVIDIA Streamline's MIT-covered portions and copied public SDK headers retain
  their original notices.
- `nvngx_dlss.dll` is a proprietary NVIDIA NGX/DLSS runtime; it is **not**
  relicensed under AGPLv3 by this repository.
- AMD FidelityFX SDK binaries are governed by the particular vendor terms
  applicable to the pinned files; they are **not** part of the NVIDIA exception.

## Approvals needed for a public combined release

1. Record an explicit permission for affected **original AGPLv3 contributions** from
   each copyright holder whose consent is needed. Maintainer status alone does
   not establish copyright ownership.
2. Confirm the scope of the proposed NVIDIA-only additional permission in
   [NVIDIA_STREAMLINE_AGPL_EXCEPTION.md](NVIDIA_STREAMLINE_AGPL_EXCEPTION.md)
   with the coauthor of #3681 and any other required rights holders.
3. Independently comply with the NVIDIA RTX/NGX SDK agreement and determine
   whether additional NVIDIA authorization or clarification is required.
4. Keep legal clearance separate from GPU/CI test status.

References:
- https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs
- https://www.gnu.org/licenses/agpl-3.0.html#section7
- https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt

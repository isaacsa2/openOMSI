# Proton Bus converter — recovery status

This directory and its supporting source are being recovered from [PR #220](https://github.com/isaacsa2/openOMSI/pull/220), last updated on 2026-10-13.

## What has been preserved

- The original `omsi-proton` converter source, examples and synthetic tests, at `crates/omsi-proton/`.
- The original 2026-10-13 branch remains available at `archive/pr220-legacy-20261013` for comparison.
- The feature recovery branch is rooted in the current official `main`, instead of replacing newer game code with October-era versions.

## Current status: source recovery only

**Not ready for runtime use, merging, or distribution as a working converter.**

The restored crate is **not yet included in the workspace** and the old integrations (launcher, installer, OI format readers, native scene, traffic, input, vehicle, navigation and localisation) have **not** been replayed over current main. This is intentional: applying October versions of shared engine files to November main would overwrite significant upstream changes. Historical generated build artifacts are also not evidence of current compatibility.

## Porting checklist

- [ ] Reconcile OI-format readers and format validation with current `omsi-cfg`, map and vehicle pipelines.
- [ ] Reconcile the converter's `omsi-proton` APIs with current workspace dependencies; regenerate lockfile and enable the crate in the workspace.
- [ ] Restore safe, opt-in **local** map/vehicle conversion in the Mods installer, without any auto-bypass.
- [ ] Port launcher/scene/navigation/AI traffic/passenger/lighting and rigid-bus hooks against current APIs, in small reviewable changes.
- [ ] Preserve mandatory `[openomsi]\nallowConversion=1` validation, source staging/rollback and path/size controls on **all** entry points.
- [ ] Keep creator attribution and source asset licenses; the editable declaration is **not** proof of authorship or redistribution permission.
- [ ] Validate supported PBSU/Ciano versions with creator-supplied test packages; do not ship models, textures, sounds or maps from third parties.
- [ ] Rework old docs to avoid implying support that is not yet present.
- [ ] Run `cargo fmt`, `cargo check`, targeted tests, size-budget, security audit and platform builds on the **new head**.
- [ ] Perform in-game testing (graphics, physics, doors, passengers, traffic and night lighting) before requesting upstream review.

## Historical validation

The 2026-10-13 test build workflows succeeded for release packaging, but PR checks failed the size budget (one extra line in `crates/omsi-launcher-core/src/lib.rs`) and the security audit reported a RustSec vulnerability. Those historical results cannot be used to approve the revived code on current main.

The original implementation lives at [commit 22c8401](https://github.com/isaacsa2/openOMSI/commit/22c8401e5d6bd868a7ee4e5ce031204eaa49552e).

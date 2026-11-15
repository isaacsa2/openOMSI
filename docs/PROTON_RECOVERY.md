# Proton Bus converter – recovery progress (15 November 2026)

This is a carefully staged revival of [PR #220](https://github.com/isaacsa2/openOMSI/pull/220) on the official `main` of November 15, 2026. The complete historical branch is backed up at `archive/pr220-legacy-20261013`.

## Restored

- The full experimental `omsi-proton` crate, map and rigid-vehicle conversion examples, source and synthetic tests.
- OI envelope decoding in `omsi-cfg::oi` and the current VFS, preserving file-size and format validation.
- Workspace dependencies and `Cargo.lock` updated for the current scene, launcher and converter graph.
- The opt-in local Proton map workflow in Mods, using selected-package validation, staging, cancellation, disk safeguards and regular install ownership.
- Recognition of generated `.cfgoi`, `.tileoi`, `.terrainoi`, `.oscoi`, `.o3doi`, `.busoi`, `.ovhoi` and `.hofoi` in the relevant readers, indexes and launcher's map/vehicle listings, where applicable.
- Map demand without a timetable (opt-in), some AI and vehicle compatibility glue, and navigation lanes from authored imported road data.
- Launcher code split into content-list, timetable, settings and installer modules to preserve the current architecture.

## Validation confirmed on GitHub Actions

- `cargo check -p omsi-proton -p omsi-cfg -p omsi-launcher-core -p omsi-sim -p omsi-vehicle -p omsi-world --all-targets` passed.
- `cargo test -p omsi-proton --lib` passed.
- `cargo test -p omsi-launcher-core install::proton::tests --lib` passed.
- `cargo test -p omsi-cfg oi::tests --lib` passed.
- A Rust size-budget pass was obtained before the latest scene edits; final checks must be rerun.

**Do not interpret these checks as proof of full gameplay support.** The full-game build, modern rendering/signal-light/vehicle integration, runtime physics, loading real creator-provided maps, and platform-specific release artifacts require validation. Newer changes to scene/navigation are not covered by the earlier focused checks. Formatting checks still flag some older touched source files.

## Next integration and validation work

- [ ] Compile a full executable for each supported platform (Windows, Linux, macOS, Android) with the updated lockfile and verify the generated artifacts
- [ ] Resolve new compiler, rustfmt and test issues without replacing current upstream engine systems or raising size budgets casually
- [ ] Bring scene, navigation, traffic, signal lighting, cabin, animations and collision features up to their present engine APIs
- [ ] Test installation and removal, memory/disk budgets, malformed inputs and cancellation across ZIP/folder sources
- [ ] Use legally supplied Proton Bus/Ciano maps and rigid buses for visual, physics and gameplay tests
- [ ] Refresh user documentation and translated strings only when supported features have been verified

## Creator consent and licensing

Every conversion entrypoint must retain canonical `[openomsi]\nallowConversion=1` validation, staging and rollback. This text is an editable declaration, **not verified proof of authorship, ownership or a grant to redistribute**. No third-party source maps, models, scripts, textures or sound files are bundled, and none may be redistributed without a valid permission or licence. The OI format is open-source format separation, **not DRM**.

**PR state: Draft / not merge-ready.**

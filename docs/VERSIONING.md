# Versioning and releases

## The version number: `MAJOR.MINOR.COMMIT`

| Part | Where it comes from | Example |
| --- | --- | --- |
| `MAJOR` | the first number in the [`VERSION`](../VERSION) file, changed by hand | `1` |
| `MINOR` | the second number in `VERSION`, changed by hand | `1.2` |
| `COMMIT` | counted automatically: commits on `main` since `VERSION` last changed (not counting those marked `[skip ci]` or `[skip actions]`, which are never released) | `1.2.17` |

`COMMIT` restarts from `0` whenever `VERSION` changes: after `1.1.23`, a commit that sets
`VERSION` to `1.2` is released as `1.2.0`, and the next one as `1.2.1`.

To start a new version, edit one line and push:

```sh
echo 1.2 > VERSION
git commit -am "Version 1.2" && git push
```

`scripts/version.sh` prints the version of the checked-out commit; the game shows it in the
launcher's side bar, in its log and in `openomsi --version` (baked in by
`crates/omsi-app/build.rs`, which uses the same rule).

## Releases

[`.github/workflows/release.yml`](../.github/workflows/release.yml) uses two publication
channels while keeping one build matrix:

1. every pull request builds the platform test artifacts without publishing a release;
2. every ordinary push to `main` builds and tests the same targets, then replaces the
   moving **`nightly` prerelease** with that commit's artifacts;
3. a maintainer runs the workflow manually (`workflow_dispatch`) to promote the selected
   current commit as the full **stable** release `v<version>`.

Both channels use `scripts/version.sh` and build Windows x64/ARM64, macOS Apple
silicon/Intel, Linux x64/ARM64, Android and the Windows/Linux dedicated-server archives.
The nightly assets still include the exact `MAJOR.MINOR.COMMIT` version in their filenames,
while the moving release/tag itself is named `nightly`.

The launcher updater keeps using GitHub's `/releases/latest` endpoint. GitHub defines that
endpoint as the newest published **non-prerelease, non-draft** release, so nightly builds do
not replace the stable update channel and no updater protocol change is required.

A stable promotion creates or updates `v<version>` with the same versioned archives and
the changelog-derived release notes. Build output never goes into the repository
(`target/` and `dist/` are ignored).

### Test builds of a pull request

A pull request's builds are test builds for anyone who wants to try the change:

* their version names the pull request, `<version>-pr<number>` (e.g. `0.1.1313-pr1192`),
  in the launcher's side bar, `game.log` and crash reports;
* the updater never offers them a release, so a test build stays until it is deleted;
* the Android one is an app of its own, *openOMSI PR #N* (`org.openomsi.game.pr`), that
  installs beside the release and shares its `openOMSI` folder;
* [`.github/workflows/pr_builds.yml`](../.github/workflows/pr_builds.yml) comments on the
  pull request with a download link per platform (through [nightly.link](https://nightly.link),
  no GitHub account needed) once they are built, and updates that comment on every push.
  The files are kept for 14 days.

The version badge at the top of the README always shows the newest release.

## The website

[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) publishes `site/` together with
the Markdown files of `docs/` to GitHub Pages
(https://openomsi-project.github.io/openOMSI/) whenever they change on `main`. The site renders
the Markdown in the browser, so a documentation change is one edit in `docs/`.

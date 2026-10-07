# Support packages

In the launcher, open **Setup → Export diagnostics**. The status shows the ZIP
saved under the application's data folder, in `diagnostics/`. Attach it manually
to an issue. Nothing is uploaded automatically.

Without a working launcher or OMSI installation:

```sh
openomsi --export-diagnostics support.zip
```

The ZIP refuses to overwrite an existing file. It contains `diagnostics.txt`,
`diagnostics.json` and privacy projections under `logs/` for available app logs.
The launcher selection and the last recorded game are labelled separately; a
session snapshot is taken at the first frame, every minute and on device loss.
The timestamp distinguishes a previous session from the current selection.
Multiple running games: the most recently started registered session is selected.

Privacy takes precedence over copying free-text logs. Only fixed event names and
the fixed stutter header's frame/time measurements survive. Chat, configuration
dumps, environment, command lines, paths, network addresses, credentials and
unknown log lines are omitted. These attachments cannot replace a privately
reviewed raw log for every error; the projection is explicitly labelled.
Each log reads at most the last 2 MiB and starts at a complete line.

Graphics fields include actual backend, adapter IDs and driver, enabled device
features/selected limits, effective MSAA/SSAO/render scale and basic pipeline
state. Requested settings are separate. Controller capabilities use the existing
device enumeration without opening an FFB effect. Unavailable fields are `null`.
Content roots and mounted archives keep order, file counts and size through
aliases, without revealing absolute paths, filenames or home usernames.
There is currently no authoritative list of loaded content packs, so that field
is `null`. The device-loss flag and fallback events are available; arbitrary
driver error text is not copied. No hardware serial is queried.

Manual verification:

- Export before any game, during a drive and after a device loss/crash.
- Test Windows, Linux, macOS and Android, including the Android Setup layout.
- Compare requested vs actual MSAA/SSAO/render scale after a renderer fallback.
- Two simultaneous sessions must select the latest session and its numbered log.
- Seed app logs with credentials, paths (including spaces/UNC), chat and IPs;
  inspect every ZIP member and verify these strings are absent.
- Verify missing/unreadable logs and absent controller data do not prevent export.
- A pre-existing output filename must be untouched.

Automated tests cover redaction, cached snapshot filtering, free-text exclusion,
ZIP entries and overwrite protection. Run `cargo test --workspace` and
`cargo build --release` before submitting upstream.

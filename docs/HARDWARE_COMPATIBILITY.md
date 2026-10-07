# Hardware compatibility matrix

openOMSI runs through several graphics backends on very different desktop and mobile
drivers. A successful run on one device does not prove that every GPU in the same family
works, and a driver crash on one phone does not prove that the whole backend is broken.

This document defines one small, repeatable hardware-report format so GPU/driver problems
can be grouped by family instead of being treated as unrelated device reports.

## Status

Use one of these values:

| Status | Meaning |
| --- | --- |
| **PASS** | Starts and completes the standard test without a compatibility workaround. |
| **DEGRADED** | Usable, but only with a fallback/backend/setting change or with a known visual/performance limitation. |
| **FAIL** | Cannot complete the standard test because of a crash, device loss, black screen or other blocking fault. |
| **UNTESTED** | Hardware is known, but the standard test has not been completed. |

## Standard test

Record the exact openOMSI version first. Then, where the platform and owned OMSI content
allow it:

1. Start the launcher twice, including one cold start after closing openOMSI.
2. Start a stock map with a stock bus.
3. Drive for at least 10 minutes with ordinary AI and passengers enabled.
4. Exercise the bus doors and mirrors.
5. Change time/weather once so the renderer rebuilds the relevant sky/lighting state.
6. End the session and start a second session without restarting the operating system.
7. Record any automatic graphics-backend fallback, device-lost event, visual corruption,
   long stall or crash.

For Android, also record the device model and SoC. For desktop systems, record the GPU and
driver version. Do not report a result as PASS if a required fallback was used; that is
DEGRADED and the workaround belongs in Notes.

The test is intentionally short. It is a compatibility smoke test, not a performance
benchmark or proof that every map/mod works.

## Results

Keep entries attributable to a GitHub report so they can be corrected when a driver or game
version changes.

| Platform | OS / device | CPU / SoC | GPU | Driver | API | Graphics mode | openOMSI | Status | Report / notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| _example_ | Windows 11 | Ryzen 5 | Radeon RX 6600 | 00.00.0 | Vulkan | Vanilla+ | 0.x.x | UNTESTED | Replace this example with linked reports |

Do not turn one row into a permanent support claim. Results are observations for one
openOMSI build, operating-system version and driver combination.

## Reporting a new combination

Use the **Hardware compatibility report** issue form. Include:

- exact openOMSI version or PR build;
- OS and OS version;
- device model on mobile, or CPU on desktop;
- exact GPU and driver version when available;
- selected graphics API and whether openOMSI fell back to another one;
- graphics mode/preset;
- PASS, DEGRADED or FAIL result;
- the standard-test steps completed and the first failing step;
- the smallest useful log excerpt for a failure.

If the build has the opt-in diagnostics export, attaching that ZIP can provide the same
hardware/backend context with less manual transcription. Inspect anything you attach first;
never post credentials, private chat, IP addresses or unrelated personal paths.

Performance comparisons belong in the profiler/performance workflow instead of this matrix:
two different GPUs cannot be ranked from these compatibility statuses.

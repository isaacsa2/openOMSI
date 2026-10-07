# AI regression coverage and remaining retests

This tests-only change extends `bus_service.rs`'s existing state machine.
No AI logic changes. EARLY_LEAVE remains 20 seconds; #1682 is not revived.

| Case | Automated coverage | Manual retest |
| --- | --- | --- |
| First/intermediate/final stop, normal trip, no passenger hold | New three-stop phase lifecycle | Actual doors and boarding at first point (#1258) |
| Early/late, 20-second boundary, layover boarding window | New boundary and phase sequence | Timetable vs on-demand stops (#1714) |
| Last loaded stop with route still open | New depart/restart sequence | Actual tile unload/reload and route extension |
| Same stop object visited again, left/right/both platforms | New restart/side sequence; existing timetable occurrence tests | Recife BRT and NCC repeated track visits (#1592) |
| Passenger hold at matching/unrelated stop | New lifecycle plus existing hold test | Boarding/alighting physical paths and missing nearby stop (#1593) |
| Trip completion and next trip | New completion/restart state assertions | Duty handover/spawning (#1544) |
| No-unscheduled traffic and pool rules | Existing sim/app permission tests (#1732 changes) | Cayuga USA (#1701) |
| Entry signals without scenery ID | Existing entry-light tests (#1733 changes) | Specific red/right-turn signals and priority (#1695) |
| Parked obstruction/merge/lane change | Existing parked-clearance and way-user tests | Narrow streets, blocked stops |
| Junction keep-clear | Existing open PR #1761 | Review/test that PR, do not duplicate it here |
| AI destination/plain aigroup_2 | Existing open PR #1757 | Review/test that PR |

Synthetic vehicle scripts are made by the existing script_test_vehicle helper.
They are not copied from OMSI assets and need no paid DLC. Test-only phase
arrival is explicit; it does not claim to exercise physical approach, passenger
walking or tile IO. All runtime claims still require the listed manual cases.

Validation required: `cargo test --workspace`, `cargo build --release`.

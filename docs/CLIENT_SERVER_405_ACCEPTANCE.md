# Unified client + native server acceptance — #405

This is an **experimental test branch**, not a production release. It combines the native dedicated server (#405), the client fixes (#408), and official upstream including #4111. PR #412 was used to resolve shared-code changes. Do not treat automated tests as equivalent to two real graphical clients.

## Automated gates

- [ ] Current-head PR checks: rustfmt, file/function size budgets, crate layering, settings/translation validation
- [ ] Current-head security: CodeQL, secret scan, audit
- [ ] Current-head headless regression suite, including private transfer policy and passenger CLAIM/BOARD/RETURN
- [ ] Current-head Release: Windows workspace tests and x64 executable, Linux x64, macOS arm64, Android build
- [ ] Investigate any failure on the exact current commit; do not soften CI thresholds or silence regression tests
- [ ] Ensure fork `main` remains aligned to official upstream before release approval

## Licensed map and two-client acceptance

1. Use **two graphical clients from the exact same build** and the native CPU-only server with a licensed local OMSI map. Confirm joining, driving, interior view, doors, route/IBIS/HOF, AI and timetable, and disconnect/reconnect.
2. Verify independent buses on the **same line, direction and stop**: separate passenger CLAIMs, physical boarding, seat occupancy, PAXBOARD acknowledgement, PAXRETURN on skipped/abandoned boardings, and no ghost or duplicate passengers. Repeat with one bus full, one bus arriving late, and both buses leaving a stop.
3. Set `pax_board_when_stopped=0`: boarding must retain original behavior. Set it to `1`: a moving bus or a bus with every entry door shut must not draw waiting passengers toward it; a stopped bus with an open entry door must. Repeat on a bus with left-side entry doors and on a modded bus.
4. On the **other** client's bus, ensure remote seated passengers maintain seated poses in original/enhanced animations; no repeated transitions after rejoining.
5. Independently toggle `pax_bad_driving_alight` and ambient passenger conversations. The original behavior must remain the default where specified; avoid duplicated voice playback on LAN mirrors.
6. Select two HOF/depot entries with overlapping names: an exact match wins, while fuzzy fallback and custom destination text remain functional.
7. Test native AI, people and scheduled journeys with a player driving. Observe stable vehicle identity, position, doors, passenger counts, spawn/despawn and route continuity across distance.
8. Test /reload, /restart, /speed, /traffic, /passengers and client reconnection using the documented supported modes. A command must not claim success without changing simulation state.
9. With a date-specific Chrono folder and `ailists` change, cross 23:59 -> 00:00. Verify the new date's tile patches, AI lists and timetable load, while the LAN session and players remain connected. Repeat after a date jump and a failed reload.
10. Explicitly disable private-content transfer (`accept_uploads = 0`, `share_maps = 0`, `allow_file_transfer = 0`, where supported). Confirm a second client cannot receive proprietary maps/vehicles/scripts via the test server. Test permission denial and reconnect.
11. Test a player-hosted LAN session separately. Its protocol and graphical host behavior must not depend on the dedicated server's headless rendering path.

## Experimental conductor cabin controls and stable passenger visibility

- [ ] On the owner's bus, Ctrl+Alt+K selects/revokes the **sole authenticated player currently aboard** as interim assistant, not every passenger. If two or more players are aboard, no assistant is selected. Permissions are off by default and revoked on the owner's disconnection/reconnect.
- [ ] With two authenticated clients, only the selected assistant inside the owner's bus can request grouped doors via the **temporary** Ctrl+Alt+1..9 shortcut while stopped. All other passengers must be rejected; normal passenger stop-bell actions remain allowed. Physical switch clicking and selecting among multiple riders are pending.
- [ ] Test Ctrl+Alt+W (first eligible window) and Ctrl+Alt+Shift+1..9 (indexed windows) on different bus models. Only recognized model mouse-event triggers should execute. Models requiring dragging must remain documented as unsupported rather than receiving unsafe arbitrary script controls.
- [ ] Verify no passenger can trigger the engine, driving controls, parking brake, private scripts, or unauthorized fare transactions. This test covers **cabin delegation only**, not ticket-selling or exclusive conductor assignment.
- [ ] In a busy district with over 96 pedestrians within the observer's interest radius, verify the closest people remain visible when internal population order changes, with no repeated GONE/spawn flicker for stationary people. Legitimate distance-based GONE is still required.
- [ ] Reorder and duplicate network WORLD packets so an older pedestrian GONE arrives after a newer position. Verify it is ignored on both native and relayed-client mirrors.
- [ ] Check a busy same-line passenger stop using two separate buses: GRANT, physical BOARD/RETURN, escrow, and on-board figures should not disappear or transfer between buses without a real event.

## Real-time clock, METAR and multiplayer Weather UI (2026-11-21)

- [ ] Use a **full process restart**, not `/reload`, to change `real_time`, `metar_sync` or `metar_station` on the native server. These are restart-only settings; `/reload` must preserve existing settings and explicitly reject them. With `metar_sync=1`, specify a four-letter `metar_station`; do not guess the nearest airport in headless mode.
- [ ] Confirm `real_time=1` matches the server machine's local date/time, including crossing midnight and an operator clock offset, without independent client clocks overriding the session.
- [ ] Confirm `metar_sync=1` reports a non-blocking, host-authoritative METAR that is refreshed and broadcast to every client. During a service outage retain the last valid weather; do not freeze the simulation tick or silently replace weather with clear skies.
- [ ] On a LAN **client**, World > Weather and Time show the host's current read-only conditions and clock. Visual precipitation, sky, wet roads and temperature should track the same weather state after the network update. Only the host/admin may edit session weather.
- [ ] Test both features OFF, each ON independently, and both ON; reconnect a late client and verify new snapshots without protocol changes.
- [ ] The **remote physical co-op cabin UI**, exclusive assistant assignment, mutual driver handover, and GPS traffic-light countdown are separate unfinished acceptance items. No passenger privilege should be granted merely by boarding.

## Exclusive co-op and GPS functionality required for completion

- [ ] Replace the temporary index/shortcut control path with actual ray/picking of existing `[mouseevent]` switches in the remote bus, including press, release and drag gestures; whitelist appropriate switch roles.
- [ ] An authenticated driver explicitly appoints **exactly one consenting assistant/conductor** to the bus. Ordinary passengers retain only normal passenger actions (e.g. a stop request); no cabin access just for being inside. Revoke on exit, switch, reconnect, disconnect and timeout.
- [ ] Allow driver and assistant to exchange roles only when stopped, with a two-party confirmation, single vehicle-authority epoch/token and recovery on connection loss; no private vehicle/mod transfer.
- [ ] Show a countdown on the GPS only when a real signal phase schedule or reliable server state is available; avoid fabricated time-to-green or extra per-frame work.

## GPU/memory and native load evidence

- Drive a large map for an extended session, change views/settings, return to unloaded areas and repeat on a low-VRAM GPU. Measure missing textures, mip restoration, load time, VRAM/RAM, peak frame times and unusual repeated uploads.
- Compare `OMSI_TEXTURE_RAM_CACHE_MB=0`, `128` and `256` (desktop only). Report actual resident RAM, VRAM, p95/p99 frame time and client crashes.
- Use the native timings scripts in `scripts/server-headless/` and `docs/DEDICATED_SERVER_TIMINGS.md`. Capture true TPS, stage p95/p99, server CPU, RSS and network data with reproducible client/player counts and identical map configuration.
- Test escalating traffic and people counts. The 512 AI-car / 2,000-person configuration values are ceilings, **not verified sustained loads**. Never report simulated or historical maxima as measurements from this branch.
- Compare with the latest upstream baseline, with the same licensed map and settings, and capture the commit SHA and system information.

## Known limits and decision

- Co-op **driver handover**, mutual role swap, physical mouse/drag controls and **conductor fare role** remain authorization scaffolding, not playable. Interim cabin permissions are limited to **one selected onboard player**, not every passenger, and still need real-client acceptance.
- Native authority over player-bus physics/scripts, full passenger capacity, modded bus boarding, rollback-safe streaming, and mixed-version client compatibility still need independent sign-off.
- Complete real-client and security acceptance before proposing merge to the fork or official main; screenshots only when they materially clarify a visual regression.

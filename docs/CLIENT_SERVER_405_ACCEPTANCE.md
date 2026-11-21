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

- [ ] On the owner's bus, Ctrl+Alt+K explicitly enables cabin control; it is disabled by default and revoked after leaving/reconnecting to LAN. Ctrl+Alt+K disables it again.
- [ ] With two authenticated clients, a rider physically inside the *other* player's bus operates grouped doors using Ctrl+Alt+1..9 while fully stopped. Doors do not operate above 0.5 km/h, from outside the bus, from another bus, or when disabled.
- [ ] Test Ctrl+Alt+W (first eligible window) and Ctrl+Alt+Shift+1..9 (indexed windows) on different bus models. Only recognized model mouse-event triggers should execute. Models requiring dragging must remain documented as unsupported rather than receiving unsafe arbitrary script controls.
- [ ] Verify no passenger can trigger the engine, driving controls, parking brake, private scripts, or unauthorized fare transactions. This test covers **cabin delegation only**, not ticket-selling or exclusive conductor assignment.
- [ ] In a busy district with over 96 pedestrians within the observer's interest radius, verify the closest people remain visible when internal population order changes, with no repeated GONE/spawn flicker for stationary people. Legitimate distance-based GONE is still required.
- [ ] Reorder and duplicate network WORLD packets so an older pedestrian GONE arrives after a newer position. Verify it is ignored on both native and relayed-client mirrors.
- [ ] Check a busy same-line passenger stop using two separate buses: GRANT, physical BOARD/RETURN, escrow, and on-board figures should not disappear or transfer between buses without a real event.

## GPU/memory and native load evidence

- Drive a large map for an extended session, change views/settings, return to unloaded areas and repeat on a low-VRAM GPU. Measure missing textures, mip restoration, load time, VRAM/RAM, peak frame times and unusual repeated uploads.
- Compare `OMSI_TEXTURE_RAM_CACHE_MB=0`, `128` and `256` (desktop only). Report actual resident RAM, VRAM, p95/p99 frame time and client crashes.
- Use the native timings scripts in `scripts/server-headless/` and `docs/DEDICATED_SERVER_TIMINGS.md`. Capture true TPS, stage p95/p99, server CPU, RSS and network data with reproducible client/player counts and identical map configuration.
- Test escalating traffic and people counts. The 512 AI-car / 2,000-person configuration values are ceilings, **not verified sustained loads**. Never report simulated or historical maxima as measurements from this branch.
- Compare with the latest upstream baseline, with the same licensed map and settings, and capture the commit SHA and system information.

## Known limits and decision

- Co-op **driver handover** and **conductor fare role** remain authorization scaffolding, not playable. Opt-in **cabin door/window controls** are experimental; they need real-client acceptance and only support model-defined click triggers.
- Native authority over player-bus physics/scripts, full passenger capacity, modded bus boarding, rollback-safe streaming, and mixed-version client compatibility still need independent sign-off.
- Complete real-client and security acceptance before proposing merge to the fork or official main; screenshots only when they materially clarify a visual regression.

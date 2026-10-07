# Modifier capture timing (#1748)

Concrete source-level reproduction: ModifiersChanged(Shift) → KeyboardInput(A pressed)
→ ModifiersChanged(empty) → controls page redraw. Main queues A but reads the live
Shift/Ctrl/Alt booleans at redraw; these are now false and the saved modifier is 0.
The first commit extracts that same calculation and adds a failing regression test.
The second snapshots the chord at key press and switches the test's event injection
to the production capture helper. It also covers Ctrl/Alt/combined chords and the
existing key-event fallback used on Android. Later modifier presses cannot replace
an already queued non-modifier key; discarded input clears the snapshot.

This environment has no Rust compiler. The test-first CI run was started, but was
still queued when the fix was prepared; its expected failure has not been observed.
CI must confirm the regression and corrected tests. This is source evidence for one
timing defect, not a hardware reproduction or proof that all of #1748 is resolved.

Upstream #1437 and #1594 were reviewed. #1437 touches the same launcher files:
coordinate the small event snapshot during integration. No binding UI redesign,
controller actions, DirectInput/FFB, scan-code mapping, hold bit, save/load format,
runtime interpretation or modifier-only binding policy is changed.

Manual matrix (pending):

| Test | Expected |
| --- | --- |
| Windows Shift/Ctrl/Alt+A; release before and after redraw | Same chord saved in both cases |
| Both left/right modifier keys; combined chord | Correct bits, no dropped modifier |
| Save, quit launcher, reload, run assigned action | Same scan code/chord; KEY_HOLD retained |
| Plain key after a chord; focus away and resume | No stale modifier |
| Modifier alone; Escape while capturing | Remains waiting; Escape cancels |
| Android keyboard without ModifiersChanged | Key-event fallback still captures chord |
| macOS Cmd text editing | Existing copy/paste and binding semantics preserved |

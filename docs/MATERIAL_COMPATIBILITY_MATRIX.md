# Material compatibility regression fixture

The synthetic configuration in `crates/omsi-model/tests/fixtures` is original test data; it uses no DLC assets and does not load its placeholder textures or mesh.

Run `cargo test --locked -p omsi-model compatibility_fixture`.

It checks preservation of ordered multiple lightmaps alongside freetex and alphascale, distinct indices for a repeated texture name, case-insensitive base-slot inheritance for matl_item, and isolation of a variant's transmap. It preserves alpha/depth directives but does not prove rendered transparency correctness.

| Issue | Automated coverage | Required manual retest |
| --- | --- | --- |
| #1650 | Lightmaps/freetex/slot/variant metadata | Switch both lighting variables independently, then both together, on affected bus; inspect texture selection and runtime material slots. |
| #1299 | Alphascale survives parsing and variant inheritance | Sweep alpha variable through 0, 0.5, 1 on current main. |
| #1492 | Alpha/depth directives retained | Compare overlapping transparent surfaces and depth writes on current main. |
| #1745 | None for rendered glass overlay | Reproduce reported 0.2.11 bus with reference screenshot, then compare main using identical backend/settings. |

No renderer, shader, parser behavior, or scenery texture logic is changed. #1765 is an open upstream PR touching scenery freetex/texttexture and must be considered before future fixes.

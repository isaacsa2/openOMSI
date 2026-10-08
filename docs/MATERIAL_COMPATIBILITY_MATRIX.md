# Material compatibility regression fixture

The synthetic configuration in `crates/omsi-model/tests/fixtures` is original test data; it uses no DLC assets and does not load its placeholder textures or mesh.

Run `cargo test --locked -p omsi-model compatibility_fixture`.
Also run `cargo test --locked -p omsi-sim synthetic_material_plan`.

It checks preservation of ordered multiple lightmaps alongside freetex and alphascale, distinct indices for a repeated texture name, case-insensitive base-slot inheritance for matl_item, and isolation of a variant's transmap. It preserves alpha/depth directives but does not prove rendered transparency correctness.
The synthetic runtime test uses the same fixture with two material slots and the
existing PropsPlan/compute_mesh_props paths. It sweeps both lighting variables,
the 0.5 threshold, alpha 0/0.5/1, variant selection 0/1, and an undeclared lighting
variable after rebuilding the plan. Texture compositing and GPU rendering still
require a manual test.

| Issue | Automated coverage | Required manual retest |
| --- | --- | --- |
| #1650 | Lightmaps/freetex/slot/variant metadata | Switch both lighting variables independently, then both together, on affected bus; inspect texture selection and runtime material slots. |
| #1299 | Alphascale survives parsing and variant inheritance | Sweep alpha variable through 0, 0.5, 1 on current main. |
| #1492 | Alpha/depth directives retained | Compare overlapping transparent surfaces and depth writes on current main. |
| #1745 | None for rendered glass overlay | Reproduce reported 0.2.11 bus with reference screenshot, then compare main using identical backend/settings. |

No renderer, shader, parser behavior, or scenery texture logic is changed. #1765 is an open upstream PR touching scenery freetex/texttexture and must be considered before future fixes.

The first CI run failed the parser fixture on Linux/Windows because the fixture
wrote `[matl_zbias]` rather than OMSI's exact `[matl_Zbias]`. The existing parser
correctly ignored the misspelled keyword; this was a fixture error, not a runtime
divergence. The fixture now uses the canonical spelling and checks every keyword
before parsing. The synthetic property-plan test passed in both jobs. Current
head validation is restarted after synchronization with official main 37d9e61b.

# Content resolver regression matrix

Run `cargo test --locked -p omsi-cfg repaint_relative_common_paths`.

The test generates a stored CTI placeholder and deflated texture placeholder in a ZIP, plus a loose installation texture. It checks backslash/slash paths, two parent components, common directory casing, filename casing, and fallback between registered roots. The placeholders are original bytes, not real textures or DLC assets.

Existing archive-inside-content tests cover overlay priority and fallback to installation/another archive. This addition uses the existing VFS/path resolver; it does not alter shaders or material semantics.

Manual retest of #1648 remains required: load the affected AI and player repaints, compare the resolved CTI/texture locations, then test loose/archive roots independently and combined. The fixture proves resolver behavior only; it does not simulate the AI/player repaint caller pipeline or establish that the reported content is fixed.

#!/usr/bin/env bash
set -euo pipefail
# Temporary diagnostic for PR #408: capture exact rustfmt output for old compact files.
for fmt_file in \
  crates/omsi-app/src/app.rs \
  crates/omsi-app/src/game_lists/dropdowns.rs \
  crates/omsi-app/src/game_lists/menu_pages.rs \
  crates/omsi-app/src/game_lists/options.rs \
  crates/omsi-app/src/offscreen/setup.rs \
  crates/omsi-app/src/settings.rs \
  crates/omsi-cfg/src/flags.rs \
  crates/omsi-launcher-core/src/lib.rs; do
    rustfmt --edition 2024 "$fmt_file"
    python3 - "$fmt_file" <<'PY'
import json
import pathlib
import sys
path = sys.argv[1]
content = pathlib.Path(path).read_text(encoding="utf-8")
print("FMT408META::" + path + "::" + str(len(content.splitlines())))
for index in range(0, len(content), 6000):
    print("FMT408V2::" + path + "::" + str(index // 6000) + "::" + json.dumps(content[index:index + 6000], ensure_ascii=True))
PY
done
exit 1

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

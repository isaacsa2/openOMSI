#!/usr/bin/env bash
set -euo pipefail
# Temporary PR 408 diagnostic. Remove before finalizing.
for f in \
crates/omsi-app/src/app.rs \
crates/omsi-app/src/game_lists/dropdowns.rs \
crates/omsi-app/src/game_lists/menu_pages.rs \
crates/omsi-app/src/game_lists/options.rs \
crates/omsi-app/src/offscreen/setup.rs \
crates/omsi-app/src/settings.rs \
crates/omsi-cfg/src/flags.rs \
crates/omsi-launcher-core/src/lib.rs; do
  cp "$f" /tmp/pr408-original.rs
  rustfmt --edition 2024 "$f"
  python3 - "$f" <<'PY'
import difflib, pathlib, sys
file = sys.argv[1]
before = pathlib.Path('/tmp/pr408-original.rs').read_text().splitlines(True)
after = pathlib.Path(file).read_text().splitlines(True)
diff = ''.join(difflib.unified_diff(before, after, fromfile='source', tofile='rustfmt'))
print('FMT408DIFF::' + file + '::lines_before=' + str(len(before)) + '::lines_after=' + str(len(after)) + '\n' + diff[:11000])
PY
done
exit 1

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

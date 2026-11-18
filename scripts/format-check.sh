#!/usr/bin/env bash
set -euo pipefail
# Temporary capture of only the two remaining rustfmt discrepancies.
for file in crates/omsi-app/src/offscreen/setup.rs crates/omsi-render/src/shaders.rs; do
  cp "$file" /tmp/pr408-before.rs
  rustfmt "$file"
  python3 - "$file" <<'PY'
import difflib, pathlib, sys
p = sys.argv[1]
a = pathlib.Path('/tmp/pr408-before.rs').read_text().splitlines(True)
b = pathlib.Path(p).read_text().splitlines(True)
print("PR408REMAIN::" + p + "\n" + ''.join(difflib.unified_diff(a,b,fromfile='before',tofile='rustfmt'))[:14000])
PY
done
exit 1

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

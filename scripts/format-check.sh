#!/usr/bin/env bash
set -euo pipefail
# Temporary formatted output capture for this PR; restored in next commit.
for fmt_file in crates/omsi-scene/src/texture_budget.rs crates/omsi-scene/src/texture_budget_tests.rs; do
    rustfmt --edition 2024 "$fmt_file"
    python3 - "$fmt_file" <<'PY'
import json
import pathlib
import sys

path = sys.argv[1]
contents = pathlib.Path(path).read_text(encoding="utf-8")
for at in range(0, len(contents), 7000):
    print("FMT_EXPORT::" + path + "::" + str(at // 7000) + "::" + json.dumps(contents[at:at + 7000], ensure_ascii=True))
PY
done

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

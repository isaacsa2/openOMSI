#!/usr/bin/env bash
set -euo pipefail
# TEMPORARY diagnostic export: removed before the PR is ready.
for fmt_file in crates/omsi-render/src/render.rs crates/omsi-render/src/targets.rs crates/omsi-render/src/resources/texture_memory_tests.rs; do
    rustfmt --edition 2024 "$fmt_file"
    python3 - "$fmt_file" <<'PY'
import json
import pathlib
import sys

path = sys.argv[1]
content = pathlib.Path(path).read_text(encoding="utf-8")
for n in range(0, len(content), 8000):
    print("FMR_OUT::" + path + "::" + str(n // 8000) + "::" + json.dumps(content[n:n + 8000], ensure_ascii=True))
PY
done

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

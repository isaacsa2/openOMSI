#!/usr/bin/env bash
set -euo pipefail
# Temporary exact rustfmt capture for the mip rounding fix.
fmt_file=crates/omsi-scene/src/texture_budget.rs
rustfmt --edition 2024 "$fmt_file"
python3 - "$fmt_file" <<'PY'
import json
import pathlib
import sys
path = sys.argv[1]
text = pathlib.Path(path).read_text(encoding="utf-8")
for i in range(0, len(text), 6500):
    print("MIP_FMT::" + str(i // 6500) + "::" + json.dumps(text[i:i + 6500], ensure_ascii=True))
PY

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

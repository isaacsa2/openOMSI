#!/usr/bin/env bash
set -euo pipefail
# Temporary formatting capture for this Draft PR, removed after applying rustfmt.
for fmt_file in \
  "crates/omsi-vehicle/src/hof.rs"; do
  rustfmt --edition 2024 "$fmt_file"
  python3 - "$fmt_file" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
s = p.read_text(encoding="utf-8")
for i in range(0,len(s),6500):
    print("HOF_FMT::" + sys.argv[1] + "::" + str(i//6500) + "::" + json.dumps(s[i:i+6500],ensure_ascii=True))
PY
done

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

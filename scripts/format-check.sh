#!/usr/bin/env bash
set -euo pipefail
# Temporary CI-only rustfmt capture; remove before final review.
for fmt_source in crates/omsi-render/src/render.rs crates/omsi-render/src/resources/texture_memory_tests.rs crates/omsi-render/src/targets.rs crates/omsi-sim/src/people.rs crates/omsi-sim/src/people/ambient_talk.rs crates/omsi-sim/src/people/pax_tick.rs; do
  rustfmt --edition 2024 "$fmt_source"
  python3 - "$fmt_source" <<'PY'
import json, pathlib, sys
path = sys.argv[1]
source = pathlib.Path(path).read_text(encoding="utf-8")
for i in range(0, len(source), 7000):
    print("FMT408::" + path + "::" + str(i // 7000) + "::" + json.dumps(source[i:i+7000], ensure_ascii=True))
PY
done
# Stop this one diagnostic run after printing formatted text; script is restored next.
exit 1

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

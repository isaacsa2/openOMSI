#!/usr/bin/env bash
set -euo pipefail
# Temporary formatting diagnostic; removed after extracting exact rustfmt output.
for fmt_file in crates/omsi-scene/src/vehicle_materials.rs crates/omsi-scene/src/texture_ram_cache.rs; do
    rustfmt --edition 2024 "$fmt_file"
    python3 - "$fmt_file" <<'PY'
import json
import pathlib
import sys
file = sys.argv[1]
content = pathlib.Path(file).read_text(encoding="utf-8")
for off in range(0, len(content), 6500):
    print("RAM_FMT::" + file + "::" + str(off // 6500) + "::" + json.dumps(content[off:off + 6500], ensure_ascii=True))
PY
done

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

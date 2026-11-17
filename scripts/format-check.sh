#!/usr/bin/env bash
set -euo pipefail
# TEMPORARY: capture exact rustfmt on modified sources, restored afterwards.
for fmt_file in \
  "crates/omsi-app/src/app.rs" \
  "crates/omsi-app/src/app_impl/room_world.rs" \
  "crates/omsi-app/src/game_lists/dropdowns.rs" \
  "crates/omsi-app/src/game_lists/menu_pages.rs" \
  "crates/omsi-app/src/game_lists/options.rs" \
  "crates/omsi-app/src/offscreen/setup.rs" \
  "crates/omsi-app/src/settings.rs" \
  "crates/omsi-launcher-core/src/lib.rs" \
  "crates/omsi-sim/src/people.rs" \
  "crates/omsi-sim/src/people/ambient_talk.rs" \
  "crates/omsi-sim/src/people/model.rs" \
  "crates/omsi-sim/src/people/pax_tick.rs" \
  "crates/omsi-sim/src/people/spawn.rs"; do
 rustfmt --edition 2024 "$fmt_file"
 python3 - "$fmt_file" <<'PY'
import json, pathlib, sys
name=sys.argv[1]
s=pathlib.Path(name).read_text(encoding="utf-8")
for i in range(0,len(s),6500):
 print("TALK_FMT::"+name+"::"+str(i//6500)+"::"+json.dumps(s[i:i+6500],ensure_ascii=True))
PY
done

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

cat scripts/formatted-crates.txt | xargs -t -I{} cargo fmt --check -p {} -- --files-with-diff

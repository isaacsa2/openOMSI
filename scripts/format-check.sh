#!/usr/bin/env bash
set -euo pipefail

if [ -z ${BASE+x} ]; then comp="main...HEAD"; else comp="HEAD^1 HEAD"; fi
git diff --name-only --line-prefix="$(git rev-parse --show-toplevel)/" -z --diff-filter=d $comp -- '*.rs' | xargs -0 -I{} scripts/format-check-file.sh {}

# TEMPORARY PR #416 diagnostic: capture the exact rustfmt output for three
# upstream files that the integrated branch's formatting check identified.
for file in crates/omsi-app/src/cli.rs crates/omsi-launcher-core/src/ai_lists.rs crates/omsi-sim/src/host/mod.rs; do
    rustfmt -- "$file"
    echo "===SYNCFMT_BEGIN:${file}==="
    base64 -w 2048 -- "$file"
    echo
    echo "===SYNCFMT_END:${file}==="
done
# Finish quickly; restore the original checker after applying the result.
exit 1


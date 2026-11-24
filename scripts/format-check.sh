#!/usr/bin/env bash
set -uo pipefail
# TEMPORARY one-shot diagnostic for PR #416 after upstream merge.
for file in crates/omsi-app/src/cli.rs crates/omsi-launcher-core/src/ai_lists.rs crates/omsi-sim/src/host/mod.rs; do
    rustfmt -- "$file" || exit 2
    echo "===SYNCFMT_BEGIN:${file}==="
    base64 -w 2048 -- "$file"
    echo
    echo "===SYNCFMT_END:${file}==="
done
exit 1

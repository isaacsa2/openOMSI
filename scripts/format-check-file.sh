#!/usr/bin/env bash
# Temporary diagnostics: show the exact rustfmt patch on GitHub's Rust runner.
set -uo pipefail
file=$1
rustfmt --config skip_children=true -- "$file"
if ! git diff --quiet -- "$file"; then
    echo "=== RUSTFMT_PATCH_BEGIN $file ==="
    git --no-pager diff -- "$file"
    echo "=== RUSTFMT_PATCH_END $file ==="
    exit 1
fi
echo "$file properly formatted"

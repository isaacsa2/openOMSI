#!/usr/bin/env bash
set -uo pipefail

file=$1
if output=$(rustfmt --check --files-with-diff -- "$file"); then
    echo "$file is properly formatted. No further action needed."
else
    if grep -Fxq -- "$file" <<< "$output"; then
        echo "$file is not formatted properly."
        # Temporary diagnostics (removed after formatting): emit the formatted
        # source in base64 chunks, allowing accurate recovery of rustfmt's result.
        rustfmt -- "$file" || true
        echo "===RUSTFMT_BEGIN:${file}==="
        base64 -w 2048 -- "$file"
        echo
        echo "===RUSTFMT_END:${file}==="
        exit 1
    else
        echo "$file is properly formatted, although its children are not. Skipping."
    fi
fi
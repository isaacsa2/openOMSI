#!/usr/bin/env bash
set -euo pipefail
cargo fmt -p omsi-server
git diff -- crates/omsi-server/src
echo "::error::Temporary format diagnostic; apply patch and restore original script"
exit 1

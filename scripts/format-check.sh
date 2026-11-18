#!/usr/bin/env bash
set -euo pipefail
cargo fmt -p omsi-server
git diff -- crates/omsi-server/src crates/omsi-server/tests
echo '::error::Temporary rustfmt diagnostic; restore the original formatter gate'
exit 1

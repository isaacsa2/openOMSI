#!/bin/sh
# Build the native renderer-free (experimental) server separately from the
# legacy openomsi --server release. Never replace the production server ZIP.
set -eu
repo="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo"
out="${1:-$repo/dist/server-headless-test}"
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null 2>&1 || {
    echo "Rust/Cargo is required (https://rustup.rs)." >&2
    exit 1
}
cargo build --locked --release -p omsi-server --bin omsi-server
mkdir -p "$out"
cp target/release/omsi-server "$out/omsi-server"
cp scripts/server-headless/server-test.cfg "$out/server-test.cfg"
cp scripts/server-headless/run-test.sh "$out/run-test.sh"
cp docs/HEADLESS_SERVER_TEST.md "$out/README.md"
cp LICENSE "$out/LICENSE"
chmod +x "$out/omsi-server" "$out/run-test.sh"
echo "Experimental native headless test package: $out"

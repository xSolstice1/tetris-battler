#!/usr/bin/env bash
# Builds the browser version into dist/web (index.html + JS loader + wasm).
# Vercel runs this on every push (see vercel.json); locally, serve dist/web with any static server.
set -euo pipefail

cd "$(dirname "$0")/.."
target_dir="${CARGO_TARGET_DIR:-target}"
out=dist/web

cargo build --release --target wasm32-unknown-unknown

rm -rf "$out"
mkdir -p "$out"
cp web/index.html web/mq_js_bundle.js "$out/"
cp "$target_dir/wasm32-unknown-unknown/release/block-brawl.wasm" "$out/"

echo "Built $out:"
ls -la "$out"

#!/usr/bin/env bash
# Builds the browser version into dist/web (index.html + JS loader + wasm).
# Deploy that folder as static files, e.g.: vercel deploy dist/web
set -euo pipefail

cd "$(dirname "$0")/.."
target_dir="${CARGO_TARGET_DIR:-target}"
out=dist/web

cargo build --release --target wasm32-unknown-unknown

mkdir -p "$out"
# Clear old build output but keep the Vercel project link (.vercel/).
find "$out" -mindepth 1 -maxdepth 1 ! -name .vercel -exec rm -rf {} +
cp web/index.html web/mq_js_bundle.js web/vercel.json "$out/"
cp "$target_dir/wasm32-unknown-unknown/release/block-brawl.wasm" "$out/"

echo "Built $out:"
ls -la "$out"

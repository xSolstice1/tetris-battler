#!/usr/bin/env bash
# Vercel install step: make sure a current Rust with the wasm target is available.
# The build image ships Rust (with rustup) at /rust/bin; only install rustup if missing.
set -euo pipefail

if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain none
fi
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# Edition 2024 needs Rust 1.85+, so don't rely on whatever version the image has.
rustup toolchain install stable --profile minimal --target wasm32-unknown-unknown
rustup default stable
rustc --version

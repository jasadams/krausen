#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo build --manifest-path engine/Cargo.toml --target wasm32-unknown-unknown --release --locked
wasm-bindgen engine/target/wasm32-unknown-unknown/release/krausen_design_preview.wasm --target web --out-dir pkg --out-name calculator
printf '%s\n' 'Ready. Run: python3 -m http.server 18040 --bind 127.0.0.1'

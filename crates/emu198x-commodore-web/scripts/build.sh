#!/usr/bin/env bash
set -euo pipefail
crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workspace="$(cd "$crate_dir/../.." && pwd)"
cd "$workspace"
if [[ -n "${WASM_BINDGEN:-}" ]]; then
  # Useful when wasm-pack cannot write its global CLI cache. The CLI version
  # must match wasm-bindgen in Cargo.lock (currently 0.2.127).
  cargo build --locked --release --target wasm32-unknown-unknown -p emu198x-commodore-web
  "$WASM_BINDGEN" "${CARGO_TARGET_DIR:-$workspace/target}/wasm32-unknown-unknown/release/emu198x_commodore_web.wasm" --target web --out-dir "$crate_dir/pkg"
else
  wasm-pack build "$crate_dir" --target web --release --out-dir pkg --no-pack --no-opt
fi

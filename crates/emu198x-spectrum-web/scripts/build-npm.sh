#!/usr/bin/env bash
# Build the npm package for @emu198x/zx-spectrum.
#
# wasm-pack derives the package name from the crate name, so `--scope emu198x`
# emits `@emu198x/emu198x-spectrum-web` — the stutter that
# 198x/decisions/crate-naming.md § "Scoped registries" calls the tool's default
# rather than a decision. The scope already carries the provenance, so the
# package name does not repeat it. This renames it.
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_name="@emu198x/zx-spectrum"

cd "$crate_dir"
wasm-pack build --target web --release --out-dir pkg --scope emu198x

python3 - "$package_name" <<'PY'
import json, sys
name = sys.argv[1]
path = "pkg/package.json"
with open(path) as f:
    pkg = json.load(f)
pkg["name"] = name
with open(path, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")
print(f"package name set to {name}")
PY

echo "built $(du -h pkg/*_bg.wasm | cut -f1) of wasm in $crate_dir/pkg"

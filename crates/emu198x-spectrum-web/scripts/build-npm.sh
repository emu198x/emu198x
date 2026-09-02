#!/usr/bin/env bash
# Build the npm package for @emu198x/zx-spectrum.
#
# Two things this does that `wasm-pack build` alone does not:
#
# 1. Embeds the 48K ROM. The crate builds without it by default so this
#    repository carries no firmware; only the published package does. See
#    knowledge/decisions/test-rom-policy.md
#    § Firmware in a published browser build.
#
# 2. Renames the package. wasm-pack derives the name from the crate, so
#    `--scope emu198x` emits `@emu198x/emu198x-spectrum-web` — the stutter
#    198x/decisions/crate-naming.md § "Scoped registries" calls the tool's
#    default rather than a decision. The scope already carries the
#    provenance; the name does not repeat it.
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_name="@emu198x/zx-spectrum"
package_version="0.1.0"

rom="${EMU198X_SPECTRUM_48K_ROM:-$HOME/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom}"

if [ ! -f "$rom" ]; then
  echo "error: no 48K ROM at $rom" >&2
  echo "Set EMU198X_SPECTRUM_48K_ROM to a 16 KiB Sinclair 48K image." >&2
  exit 1
fi

# Checked here as well as in the crate's tests, because a wrong image compiles
# perfectly and then fails to boot in a browser with nothing to explain why.
size=$(wc -c < "$rom" | tr -d ' ')
if [ "$size" -ne 16384 ]; then
  echo "error: $rom is $size bytes; a 48K ROM is 16384" >&2
  exit 1
fi

cd "$crate_dir"
EMU198X_SPECTRUM_48K_ROM="$rom" \
  wasm-pack build --target web --release --out-dir pkg --scope emu198x \
  -- --features bundled-rom

python3 - "$package_name" "$package_version" <<'PY'
import json, sys
name, version = sys.argv[1], sys.argv[2]
path = "pkg/package.json"
with open(path) as f:
    pkg = json.load(f)
pkg["name"] = name
# Versioned on its own API rather than the emulator's: this package has had
# one consumer, and 0.20.x would imply twenty releases of history it has not
# had. The emulator build it came from is recorded separately.
pkg["version"] = version
with open(path, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")
print(f"{name}@{version}")
PY

echo "built $(du -h pkg/*_bg.wasm | cut -f1) of wasm in $crate_dir/pkg"

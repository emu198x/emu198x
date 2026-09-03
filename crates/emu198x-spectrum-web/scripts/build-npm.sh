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
publish=0
if [ "${1:-}" = "--publish" ]; then
  publish=1
  shift
fi

# Name and version come from the crate's own manifest, not from here. A version
# in a shell script drifts from the thing it versions, and the last publish
# needed this line edited by hand — exactly the step a person forgets.
read_meta() {
  python3 - "$crate_dir/Cargo.toml" "$1" <<'META'
import re, sys
key = sys.argv[2]
text = open(sys.argv[1]).read()
parts = text.split("[package.metadata.npm]", 1)
if len(parts) < 2:
    sys.exit("Cargo.toml has no [package.metadata.npm] section")
body = re.split(r"\n\[", parts[1], maxsplit=1)[0]
found = re.search(r'^' + key + r'\s*=\s*"([^"]+)"', body, re.M)
if not found:
    sys.exit("[package.metadata.npm] has no " + key)
print(found.group(1))
META
}

package_name="$(read_meta name)"
package_version="$(read_meta version)"

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

# The crate is GPL-2.0-or-later and wasm-pack copies that through, but this
# artifact is not: it also contains the Sinclair 48K ROM, which is copyright
# Amstrad and redistributed by permission, not relicensed. Leaving a bare
# SPDX id here tells every licence scanner the firmware is GPL, which is a
# false statement that tooling then acts on. npm's own escape hatch for a
# package that is not one licence is to point at the file that explains it.
pkg["license"] = "SEE LICENSE IN README.md"

# npm always publishes the README, but naming it makes the intent visible to
# anyone reading this manifest and stops a future `files` edit dropping the
# acknowledgement Amstrad asked for.
files = pkg.get("files", [])
if "README.md" not in files:
    files.append("README.md")
pkg["files"] = files
with open(path, "w") as f:
    json.dump(pkg, f, indent=2)
    f.write("\n")
print(f"{name}@{version}")
PY

echo "built $(du -h pkg/*_bg.wasm | cut -f1) of wasm in $crate_dir/pkg"

# Publishing from here rather than leaving a `cd pkg && npm publish` for a
# person to get right. The crate directory has no package.json, so running npm
# in the obvious place does nothing useful and explains little about why.
if [ "$publish" -eq 1 ]; then
  if [ ! -f pkg/package.json ] || [ ! -f pkg/README.md ]; then
    echo "error: pkg/ is missing package.json or README.md; not publishing" >&2
    exit 1
  fi

  cd pkg
  # Already there? Say so and stop. npm would refuse anyway, but with an error
  # that fails a workflow running on every push — and "this version is already
  # published" is a normal outcome for a reconciling job, not a fault.
  if npm view "$package_name@$package_version" version >/dev/null 2>&1; then
    echo "already published: $package_name@$package_version"
    exit 0
  fi

  # --access public because a scoped package defaults to restricted, and a
  # restricted publish looks identical in the output until an install fails.
  npm publish --access public

  # Then wait for the registry to serve it. A new package's first version can
  # take minutes to appear, and "is it there yet" is a question this should
  # answer rather than a person refreshing a page.
  echo "waiting for $package_name@$package_version to be served..."
  for _ in $(seq 1 60); do
    if npm view "$package_name@$package_version" version >/dev/null 2>&1; then
      echo "live: $package_name@$package_version"
      exit 0
    fi
    sleep 10
  done
  echo "warning: publish reported success but the registry is not serving it yet." >&2
  echo "Usually propagation. Confirm with: npm view $package_name versions" >&2
fi

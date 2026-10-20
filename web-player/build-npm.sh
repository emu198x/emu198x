#!/usr/bin/env bash
# Build the browser player as the npm package @emu198x/player, and with
# --publish, publish it.
#
#   web-player/build-npm.sh VERSION [--publish]
#
# VERSION is the Emu198x release version (0.38.4 for v0.38.4): the player is
# published at its release's version (umbrella
# decisions/browser-bundles-ship-as-npm-packages.md). The Spectrum 48K ROM must
# be supplied through EMU198X_SPECTRUM_48K_ROM, as for @emu198x/zx-spectrum; a
# package without it would have the sites ask readers for firmware, so this
# refuses to build one (knowledge/decisions/test-rom-policy.md).
set -euo pipefail

version=${1:?usage: web-player/build-npm.sh VERSION [--publish]}
publish=0
[ "${2:-}" = "--publish" ] && publish=1
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "error: VERSION must be x.y.z, not '$version'" >&2; exit 1; }
[ -n "${EMU198X_SPECTRUM_48K_ROM:-}" ] || { echo "error: set EMU198X_SPECTRUM_48K_ROM; the player package always carries the 48K ROM" >&2; exit 1; }

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
out="$root/target/npm/browser-player"
rm -rf "$out"

node scripts/build-browser-player.mjs "$out"

# The release build must be the committed tree and must carry the ROM.
node --input-type=module - "$out/build.json" "$version" <<'JS'
import {readFileSync, writeFileSync} from 'node:fs';
const [file, version] = process.argv.slice(2);
const build = JSON.parse(readFileSync(file, 'utf8'));
if (build.modified) throw new Error('the working tree has uncommitted changes; publish from a clean release checkout');
if (!build.bundledSpectrum48kFirmware) throw new Error('the Spectrum module does not carry the 48K ROM');
writeFileSync(file, JSON.stringify({...build, version}, null, 2) + '\n');
JS

cp web-player/npm/README.md "$out/README.md"
node --input-type=module - "$out/package.json" "$version" <<'JS'
import {writeFileSync} from 'node:fs';
const [file, version] = process.argv.slice(2);
writeFileSync(file, JSON.stringify({
  name: '@emu198x/player',
  version,
  description: 'The Emu198x browser player: every runtime family, the machine catalogue and demos, ready to serve from your own site.',
  license: 'SEE LICENSE IN README.md',
  homepage: 'https://emu198x.github.io/',
  repository: {type: 'git', url: 'git+https://github.com/emu198x/emu198x.git', directory: 'web-player'},
  keywords: ['emulator', 'webassembly', 'retro', 'zx-spectrum', 'commodore', 'amiga', 'nes'],
}, null, 2) + '\n');
JS

echo "built @emu198x/player@$version: $(find "$out" -type f | wc -l | tr -d ' ') files, $(du -sh "$out" | cut -f1)"

if [ "$publish" -eq 1 ]; then
  cd "$out"
  if npm view "@emu198x/player@$version" version >/dev/null 2>&1; then
    echo "already published: @emu198x/player@$version"
    exit 0
  fi
  npm publish --access public
  for _ in $(seq 1 60); do
    npm view "@emu198x/player@$version" version >/dev/null 2>&1 && { echo "live: @emu198x/player@$version"; exit 0; }
    sleep 10
  done
  echo "warning: published, but the registry isn't serving @emu198x/player@$version yet" >&2
fi

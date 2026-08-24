#!/usr/bin/env bash
# Capture ZX80 and ZX81 boot screens from MAME, for cross-checking the
# Emu198x goldens against an independent implementation.
#
# MAME is used the way FS-UAE is used for the Amiga boot goldens: an external
# emulator whose output is compared against ours. It is not hardware, and this
# script does not pretend otherwise -- see knowledge/processes/golden-image-capture.md.
#
# Everything MAME writes goes to a temporary directory. Run from a git
# worktree without this and it leaves cfg/ behind in the working tree.
#
# MAME wants its own ROM zips. Both machines' ROMs are the ones already staged
# for Emu198x's own tests, repackaged here; nothing is downloaded.
#
#   zx80.rom  sha1 b6769a3197c77009e0933e038c15b43cf4c98c7a  = MAME "zx80.rom"
#   zx81.rom  sha1 7b143ee964e9ada89d1f9e88f0bd48d919184cfc  = MAME "zx81a.rom", bios "2nd"
#
# The ZX81 BIOS matters: MAME's default is the 3rd revision and ours is the
# 2nd. Comparing against the default compares two different ROMs.
set -euo pipefail

OUT="${1:-./zx8x-mame-capture}"
ROMS="${EMU198X_ROMS:-$HOME/.emu198x/roms}"
SECONDS_TO_RUN="${SECONDS_TO_RUN:-12}"

command -v mame >/dev/null || { echo "mame not on PATH" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/roms" "$work/cfg" "$work/nvram" "$OUT"

cp "$ROMS/sinclair-zx80/zx80.rom" "$work/roms/zx80.rom"
(cd "$work/roms" && zip -q zx80.zip zx80.rom && rm zx80.rom)
cp "$ROMS/sinclair-zx81/zx81.rom" "$work/roms/zx81a.rom"
(cd "$work/roms" && zip -q zx81.zip zx81a.rom && rm zx81a.rom)

run() {
  local sys="$1"; shift
  mame "$sys" "$@" \
    -rompath "$work/roms" -snapshot_directory "$work/snap" \
    -cfg_directory "$work/cfg" -nvram_directory "$work/nvram" \
    -inipath "$work" -homepath "$work" \
    -video none -sound none -nothrottle -skip_gameinfo \
    -seconds_to_run "$SECONDS_TO_RUN" >/dev/null
  cp "$(ls -1 "$work/snap/$sys"/*.png | tail -1)" "$OUT/$sys-boot-mame.png"
  echo "wrote $OUT/$sys-boot-mame.png"
}

run zx80
run zx81 -bios 2nd

mame -version | head -1 > "$OUT/mame-version.txt"
echo "captured with $(cat "$OUT/mame-version.txt")"

#!/bin/sh
# Rebuild the ares PPU harness and rewrite the golden files.
#
#   ARES=/path/to/ares tests/oracle/regenerate.sh
#
# ARES is a checkout of https://github.com/ares-emulator/ares; the goldens
# were made at c76aaffa6e69778baf9a66d1da04298ac2ed7d1d. The harness
# compiles ares's accurate PPU sources from a copy with one patch, the
# Mode 4 offset-per-tile fix described in ppu_harness.cpp.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
ares=${ARES:?set ARES to an ares checkout}
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT

cp -r "$ares/ares/sfc/ppu" "$build/ppu"
# bsnes, and the hardware: Mode 4's vertical offset comes from the one
# offset word BG3 fetched.
sed -i 's|          voffset = vpixel + (vlookup);|          voffset = vpixel + (hlookup);|' "$build/ppu/background.cpp"
grep -q 'voffset = vpixel + (hlookup);' "$build/ppu/background.cpp"

c++ -std=c++20 -O2 -w \
  -I"$ares/nall" -I"$ares/ares" -I"$build/ppu" \
  "$here/ppu_harness.cpp" -o "$build/ppu_harness"

mkdir -p "$here/golden"
rm -f "$here/golden"/*.txt
for case in "$here"/cases/*.ppu; do
  name=$(basename "$case" .ppu)
  "$build/ppu_harness" "$case" > "$here/golden/$name.txt"
done
echo "wrote $(ls "$here/golden" | wc -l) golden files"

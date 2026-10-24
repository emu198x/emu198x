#!/usr/bin/env bash
# Rebuild the golden files from ares's S-DSP.
#
#   tests/oracle/regenerate.sh /path/to/ares     (or set ARES)
#
# ARES is a checkout of https://github.com/ares-emulator/ares (ISC licence);
# the golden files were made from commit c76aaffa6e69778baf9a66d1da04298ac2ed7d1d.
# The harness compiles ares's own DSP source (sfc/dsp/*.cpp) with a cut-down
# struct; DSP::main, the 32-step schedule, is extracted from dsp.cpp here so
# the oracle runs ares's schedule, not a copy of it.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
ares="${1:-${ARES:?pass an ares checkout, or set ARES}}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

python3 "$here/make_cases.py" "$here/cases"

awk '/^auto DSP::main\(\) -> void \{/{on=1} on{print} on&&/^\}$/{exit}' \
  "$ares/ares/sfc/dsp/dsp.cpp" > "$work/ares_main.inc"
grep -q 'voice5(voice\[0\])' "$work/ares_main.inc"

g++ -std=c++20 -O2 -w \
  -I "$ares/nall" -I "$ares/ares" -I "$work" \
  -o "$work/harness" "$here/ares_harness.cpp"

mkdir -p "$here/golden"
for case in "$here"/cases/*.dsp; do
  name="$(basename "$case" .dsp)"
  "$work/harness" "$case" > "$here/golden/$name.txt"
  echo "$name: $(wc -l < "$here/golden/$name.txt") lines"
done

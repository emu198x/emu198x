#!/usr/bin/env bash
# Differential validation of the `softfloat` port's FGETEXP / FGETMAN / FSCALE
# (and, later, FREM / FMOD / the transcendentals) against WinUAE's SoftFloat —
# the silicon-validated 68881/2 FPSP reference (Grabher / Previous lineage).
#
# Usage:   ./run_fpsp.sh [count]      (default 200000 vectors per op)
#
# Why a second oracle: Musashi's softfloat.c (run.sh) lacks the 68k-specific
# FGETEXP/FGETMAN/FSCALE/FREM/FMOD and the transcendentals, and its FMOD/FGETEXP
# are inaccurate. WinUAE vendors Grabher's hardware-validated softfloat, which
# implements them exactly as the 68881/2 does. Only result VALUES are compared
# here (see winuae_check.cpp) — the 2a-vs-2b flag layout differs.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# emulators/ is a sibling of Emu198x under the 198x umbrella.
umbrella_root="$(cd "$here/../../../.." && pwd)"
sf="$umbrella_root/emulators/amiga/WinUAE/softfloat"
count="${1:-200000}"

if [ ! -d "$sf" ]; then
  echo "WinUAE softfloat source not found at: $sf" >&2
  exit 2
fi

build="$(mktemp -d)"
trap 'rm -rf "$build"' EXIT
mkdir -p "$build/softfloat"
cp "$sf/softfloat.cpp" "$sf/softfloat.h" "$sf/softfloat-specialize.h" \
   "$sf/SOFTFLOAT-MACROS.H" "$build/softfloat/"

# softfloat.h forward-declares float_raise(), but softfloat-specialize.h then
# defines it `static inline` — a linkage clash in C++. Drop the forward decl
# (the definition is pulled in via #include before any use).
gsed -i 's/^void float_raise(uint8_t flags, float_status \*status);/\/* float_raise: defined static inline in softfloat-specialize.h *\//' \
  "$build/softfloat/softfloat.h"

# Case-insensitive macOS resolves "softfloat-macros.h" -> SOFTFLOAT-MACROS.H
# but warns; silence just that portability warning.
c++ -O2 -std=c++17 -Wno-nonportable-include-path -I"$build" \
  "$here/winuae_check.cpp" "$build/softfloat/softfloat.cpp" -o "$build/winuae_check"

# Asserted (bit-exact against WinUAE): arithmetic, conversions, FSGLMUL/FSGLDIV,
# and FGETEXP/FGETMAN/FSCALE. FREM/FMOD + the transcendentals are the remaining
# ops; inspect one by passing its op number as a second arg.
asserted=(0:add 1:sub 2:mul 3:div 4:sqrt 5:to_int32 6:to_f32 7:to_f64 \
          8:int32_to 9:f32_to 10:f64_to 11:sglmul 12:sgldiv \
          20:getexp 21:getman 22:scale)
status=0
for entry in "${asserted[@]}"; do
  op="${entry%%:*}"; name="${entry##*:}"
  printf 'op %2s (%-7s): ' "$op" "$name"
  if cargo run -q --release --example sf_gen -p motorola-68k-common -- "$op" "$count" 2>/dev/null \
       | "$build/winuae_check" 2>&1 | tail -1; then :; else status=1; fi
done

# Optional diagnostic op for re-base work (second CLI arg).
if [ "${2:-}" != "" ]; then
  printf 'diag op %s: ' "$2"
  cargo run -q --release --example sf_gen -p motorola-68k-common -- "$2" "$count" 2>/dev/null \
    | "$build/winuae_check" 2>&1 | tail -3 || true
fi
exit "$status"

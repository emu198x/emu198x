#!/usr/bin/env bash
# Differential validation of the `softfloat` port's results AND exception
# flags against the vendored Berkeley SoftFloat C source (the same library
# Musashi uses, and our oracle). For each operation, `sf_gen` generates random
# floatx80 vectors, runs the Rust port, and emits inputs + (value, flags);
# `sf_check` re-runs the C and reports any (value, flags) mismatch.
#
# Usage:   ./run.sh [count]      (default 200000 vectors per op)
#
# Why this exists: Musashi's m68kfpu.c never populates the FPSR exception
# bytes, so the single-step corpus cannot validate them. softfloat.c *does*
# compute float_exception_flags, so it is the oracle for the flag computation;
# the flag→FPSR mapping is covered by unit tests.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate_root="$(cd "$here/.." && pwd)"
repo_root="$(cd "$crate_root/../.." && pwd)"
sf="$repo_root/tools/m68k-test-gen/musashi/softfloat"
count="${1:-200000}"

build="$(mktemp -d)"
trap 'rm -rf "$build"' EXIT
mkdir -p "$build/softfloat"
cp "$sf"/softfloat.c "$sf"/softfloat.h "$sf"/softfloat-specialize \
   "$sf"/softfloat-macros "$sf"/mamesf.h "$sf"/milieu.h "$build/softfloat/"

# Stub the m68kcpu.h softfloat.c expects (base integer types + INLINE).
cat > "$build/m68kcpu.h" <<'EOF'
#ifndef SF_STUB_M68KCPU_H
#define SF_STUB_M68KCPU_H
typedef signed char sint8; typedef signed short sint16; typedef signed int sint32;
typedef unsigned char uint8; typedef unsigned short uint16; typedef unsigned int uint32;
typedef signed long long sint64; typedef unsigned long long uint64;
#define INLINE static inline
#include "softfloat/milieu.h"
#include "softfloat/softfloat.h"
#endif
EOF

cc -O2 -I"$build" "$here/sf_check.c" "$build/softfloat/softfloat.c" -o "$build/sf_check"

ops=(0:add 1:sub 2:mul 3:div 4:sqrt 5:to_int32 6:to_f32 7:to_f64 8:int32_to 9:f32_to 10:f64_to)
status=0
for entry in "${ops[@]}"; do
  op="${entry%%:*}"; name="${entry##*:}"
  printf 'op %2s (%-9s): ' "$op" "$name"
  if cargo run -q --release --example sf_gen -p motorola-68k-common -- "$op" "$count" 2>/dev/null \
       | "$build/sf_check" 2>&1 | tail -1; then :; else status=1; fi
done
exit "$status"

#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mkdir -p target/llvm-cov

coverage_toolchain="+nightly"
coverage_flags=(--branch)

# The motorola-68000 family was split into per-variant crates in April
# 2026: motorola-68k-common, motorola-68000, motorola-68010,
# motorola-68020, motorola-68030, and motorola-68040. The previous
# Cov-3 carve-out for `motorola-68000/src/(fpu|mmu|disasm)\.rs` no
# longer applies — disasm.rs was deleted, fpu.rs moved to
# motorola-68040, and mmu.rs moved to motorola-68k-common (and is
# re-exported from motorola-68030). The dormant variant crates carry
# their own coverage numbers and don't artificially depress the
# motorola-68000 figure. Tom Harte continues to validate the live
# 68000 opcode surface at 1,000,058/1,000,058.

cargo "${coverage_toolchain}" llvm-cov "${coverage_flags[@]}" \
    --workspace "$@" | tee target/llvm-cov/coverage-summary.txt
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --json --summary-only --output-path target/llvm-cov/coverage-summary.json
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --lcov --output-path target/llvm-cov/lcov.info
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --html --output-dir target/llvm-cov

echo
echo "Coverage total:"
grep '^TOTAL' target/llvm-cov/coverage-summary.txt | tail -n 1
echo
echo "Reports:"
echo "  target/llvm-cov/coverage-summary.txt"
echo "  target/llvm-cov/coverage-summary.json"
echo "  target/llvm-cov/lcov.info"
echo "  target/llvm-cov/html/index.html"

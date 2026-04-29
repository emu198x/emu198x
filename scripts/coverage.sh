#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mkdir -p target/llvm-cov

coverage_toolchain="+nightly"
coverage_flags=(--branch)

# Files we deliberately carve out of the workspace coverage number.
# These are not "uncovered code we should test" — they are code paths
# that the active machines cannot reach.
#
# - motorola-68000/src/(fpu|mmu|disasm)\.rs
#     The 68040 FPU, the 68030/040 MMU, and the disassembler. The
#     A500 (our only 68000-class target) has no FPU and no MMU; the
#     disassembler is a debugging aid that the running emulator
#     never invokes. Tom Harte validates the actual 68000 opcode
#     surface at 1,000,058/1,000,058. See Cov-3 in
#     docs/plans/2026-04-28-october-runup-plan.md.
coverage_ignore_regex='motorola-68000/src/(fpu|mmu|disasm)\.rs'

cargo "${coverage_toolchain}" llvm-cov "${coverage_flags[@]}" \
    --ignore-filename-regex "${coverage_ignore_regex}" \
    --workspace "$@" | tee target/llvm-cov/coverage-summary.txt
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --ignore-filename-regex "${coverage_ignore_regex}" \
    --json --summary-only --output-path target/llvm-cov/coverage-summary.json
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --ignore-filename-regex "${coverage_ignore_regex}" \
    --lcov --output-path target/llvm-cov/lcov.info
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" \
    --ignore-filename-regex "${coverage_ignore_regex}" \
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

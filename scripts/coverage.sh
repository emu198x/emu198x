#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mkdir -p target/llvm-cov

coverage_toolchain="+nightly"
coverage_flags=(--branch)

cargo "${coverage_toolchain}" llvm-cov "${coverage_flags[@]}" --workspace "$@" | tee target/llvm-cov/coverage-summary.txt
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" --json --summary-only --output-path target/llvm-cov/coverage-summary.json
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" --lcov --output-path target/llvm-cov/lcov.info
cargo "${coverage_toolchain}" llvm-cov report "${coverage_flags[@]}" --html --output-dir target/llvm-cov

echo
echo "Coverage total:"
grep '^TOTAL' target/llvm-cov/coverage-summary.txt | tail -n 1
echo
echo "Reports:"
echo "  target/llvm-cov/coverage-summary.txt"
echo "  target/llvm-cov/coverage-summary.json"
echo "  target/llvm-cov/lcov.info"
echo "  target/llvm-cov/html/index.html"

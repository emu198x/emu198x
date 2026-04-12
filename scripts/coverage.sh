#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mkdir -p target/llvm-cov

cargo llvm-cov --workspace "$@" | tee target/llvm-cov/coverage-summary.txt
cargo llvm-cov report --json --summary-only --output-path target/llvm-cov/coverage-summary.json
cargo llvm-cov report --lcov --output-path target/llvm-cov/lcov.info
cargo llvm-cov report --html --output-dir target/llvm-cov

echo
echo "Coverage total:"
grep '^TOTAL' target/llvm-cov/coverage-summary.txt | tail -n 1
echo
echo "Reports:"
echo "  target/llvm-cov/coverage-summary.txt"
echo "  target/llvm-cov/coverage-summary.json"
echo "  target/llvm-cov/lcov.info"
echo "  target/llvm-cov/html/index.html"

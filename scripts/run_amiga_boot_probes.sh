#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "$REPO_ROOT"

export RUST_TEST_THREADS=1

if [[ $# -gt 0 ]]; then
    cargo test -p machine-amiga --test boot_probe "$@" -- --ignored --nocapture
else
    cargo test -p machine-amiga --test boot_probe -- --ignored --nocapture
fi

echo
echo "Probe reports are written to test_output/amiga/probes/"

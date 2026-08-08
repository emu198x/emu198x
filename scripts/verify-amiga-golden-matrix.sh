#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if (($# > 0)); then
    case "$1" in
        --help|-h)
            if (($# == 1)); then
                echo "Usage: scripts/verify-amiga-golden-matrix.sh"
                exit 0
            fi
            ;;
    esac
    echo "error: unexpected arguments: $*" >&2
    exit 2
fi

PYTHONDONTWRITEBYTECODE=1 python3 -B scripts/verify-amiga-closure-assets.py \
    --lane golden-matrix

EMU198X_REQUIRE_GOLDEN_ASSETS=1 cargo test --locked --release \
    -p runtime-commodore-amiga \
    --test golden_matrix \
    -- \
    --nocapture \
    --test-threads=1

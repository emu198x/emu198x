#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if (($# > 0)); then
    case "$1" in
        --help|-h)
            if (($# == 1)); then
                echo "Usage: scripts/verify-amiga-catalogue.sh"
                exit 0
            fi
            ;;
    esac
    echo "error: unexpected arguments: $*" >&2
    exit 2
fi

PYTHONDONTWRITEBYTECODE=1 python3 -B scripts/verify-amiga-closure-assets.py \
    --lane catalogue-ten

cargo run --locked --release -q -p emu198x-catalogue -- \
    run \
    --manifest crates/emu198x-catalogue/manifest/amiga.toml

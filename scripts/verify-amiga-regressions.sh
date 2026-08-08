#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

usage() {
    cat <<'USAGE'
Usage: scripts/verify-amiga-regressions.sh

Runs the bounded, hermetic Amiga regression set used by the accuracy-closure
runner. The set comprises the declared library packages plus named integration
tests for arbitration, disk DMA and persistence, incremental blitting,
inspection, runtime lifecycle, interrupt snapshots, and catalogue structure.
USAGE
}

if (($# == 1)); then
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
elif (($# > 1)); then
    echo "error: unexpected arguments: $*" >&2
    usage >&2
    exit 2
fi

run_group() {
    local description="$1"
    shift
    printf '\n== %s ==\n' "${description}"
    "$@"
}

run_group "Amiga library regressions" \
    cargo test --locked --lib \
        -p common-commodore-amiga \
        -p motorola-68k-common \
        -p motorola-68000 \
        -p motorola-68010 \
        -p motorola-68020 \
        -p motorola-68030 \
        -p motorola-68040 \
        -p commodore-agnus-ocs \
        -p commodore-agnus-ecs \
        -p commodore-agnus-aga \
        -p commodore-denise-ocs \
        -p commodore-denise-ecs \
        -p commodore-denise-aga \
        -p commodore-paula-8364 \
        -p commodore-amiga-autoconfig \
        -p commodore-gary \
        -p commodore-gayle \
        -p mos-cia-8520 \
        -p peripheral-commodore-amiga-floppy \
        -p peripheral-commodore-amiga-keyboard \
        -p machine-commodore-amiga-ocs \
        -p machine-commodore-amiga-ecs \
        -p machine-commodore-amiga-a1200 \
        -p runtime-commodore-amiga

run_group "Agnus arbitration and blitter integrations" \
    cargo test --locked -p commodore-agnus-ocs \
        --test arbitration \
        --test blitter \
        --test blitter_startup

run_group "Paula disk integration" \
    cargo test --locked -p commodore-paula-8364 --test disk

run_group "Floppy mechanism and MFM integrations" \
    cargo test --locked -p peripheral-commodore-amiga-floppy \
        --test drive_state \
        --test mfm_adf

run_group "OCS machine arbitration, disk and blitter integrations" \
    cargo test --locked -p machine-commodore-amiga-ocs \
        --test chip_bus_arbitration \
        --test disk_dma_arbitration \
        --test dsk_write_back \
        --test blitter_register_writes \
        --test incremental_blitter

run_group "ECS machine incremental-blitter integration" \
    cargo test --locked -p machine-commodore-amiga-ecs --test incremental_blitter

run_group "AGA machine incremental-blitter integration" \
    cargo test --locked -p machine-commodore-amiga-a1200 --test incremental_blitter

run_group "Amiga runtime integrations" \
    cargo test --locked -p runtime-commodore-amiga \
        --test dsk_writable_mount \
        --test queries \
        --test a1200_interrupt_snapshot \
        --test lifecycle

run_group "Amiga catalogue manifest contract" \
    cargo test --locked -p emu198x-catalogue --test amiga_manifest

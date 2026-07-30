#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

usage() {
    cat <<'USAGE'
Usage: scripts/verify-amiga-programmable-hblank.sh

Builds the emulator-neutral programmable-HBLANK corpus and runs its explicit
full-ECS and A1200 cross-family-consensus and disagreement-measurement gate.

Environment:
    EMU198X_AMIGA_KICKSTART_204_ROM       Kickstart 2.04 r37.175 ROM
    EMU198X_AMIGA_KICKSTART_31_A1200_ROM  A1200 Kickstart 3.1 r40.068 ROM
    EMU198X_AMIGA_ROM_DIR                 fallback ROM directory
    CARGO_TARGET_DIR                      optional Cargo build directory

When explicit ROM paths are absent, the script reads kick204.rom and
kick31a1200.rom from EMU198X_AMIGA_ROM_DIR, or from
~/.emu198x/roms/commodore-amiga by default.

The corpus build requires python3, m68k-elf-as, and m68k-elf-ld. Missing or
mismatched inputs fail the gate.
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

rom_dir="${EMU198X_AMIGA_ROM_DIR:-}"
if [[ -z "${rom_dir}" && -n "${HOME:-}" ]]; then
    rom_dir="${HOME}/.emu198x/roms/commodore-amiga"
fi

kickstart_204_source="${EMU198X_AMIGA_KICKSTART_204_ROM:-}"
if [[ -z "${kickstart_204_source}" ]]; then
    if [[ -z "${rom_dir}" ]]; then
        echo "error: set EMU198X_AMIGA_KICKSTART_204_ROM or EMU198X_AMIGA_ROM_DIR" >&2
        exit 1
    fi
    kickstart_204_source="${rom_dir}/kick204.rom"
fi

kickstart_31_a1200_source="${EMU198X_AMIGA_KICKSTART_31_A1200_ROM:-}"
if [[ -z "${kickstart_31_a1200_source}" ]]; then
    if [[ -z "${rom_dir}" ]]; then
        echo "error: set EMU198X_AMIGA_KICKSTART_31_A1200_ROM or EMU198X_AMIGA_ROM_DIR" >&2
        exit 1
    fi
    kickstart_31_a1200_source="${rom_dir}/kick31a1200.rom"
fi

if [[ ! -f "${kickstart_204_source}" ]]; then
    echo "error: Kickstart 2.04 ROM not found at ${kickstart_204_source}" >&2
    exit 1
fi
if [[ ! -f "${kickstart_31_a1200_source}" ]]; then
    echo "error: A1200 Kickstart 3.1 ROM not found at ${kickstart_31_a1200_source}" >&2
    exit 1
fi

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/emu198x-amiga-programmable-hblank.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT

dist_dir="${work_dir}/dist"
firmware_dir="${work_dir}/firmware"
mkdir -p "${firmware_dir}"

normalized_kickstart_204="${firmware_dir}/kick204.rom"
normalized_kickstart_31_a1200="${firmware_dir}/kick31a1200.rom"
cp "${kickstart_204_source}" "${normalized_kickstart_204}"
cp "${kickstart_31_a1200_source}" "${normalized_kickstart_31_a1200}"

if command -v sha256sum >/dev/null 2>&1; then
    sha256_file() {
        sha256sum "$1" | awk '{print $1}'
    }
elif command -v shasum >/dev/null 2>&1; then
    sha256_file() {
        shasum -a 256 "$1" | awk '{print $1}'
    }
else
    echo "error: sha256sum or shasum is required to validate firmware" >&2
    exit 1
fi

validate_rom() {
    local path="$1"
    local description="$2"
    local expected_sha256="$3"
    local actual_bytes
    local actual_sha256

    actual_bytes="$(wc -c < "${path}" | tr -d '[:space:]')"
    if [[ "${actual_bytes}" != "524288" ]]; then
        echo "error: ${description} has ${actual_bytes} bytes; expected 524288" >&2
        exit 1
    fi

    actual_sha256="$(sha256_file "${path}")"
    if [[ "${actual_sha256}" != "${expected_sha256}" ]]; then
        echo "error: ${description} SHA-256 is ${actual_sha256}; expected ${expected_sha256}" >&2
        exit 1
    fi
}

validate_rom \
    "${normalized_kickstart_204}" \
    "Kickstart 2.04 r37.175 ROM" \
    "d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79"
validate_rom \
    "${normalized_kickstart_31_a1200}" \
    "A1200 Kickstart 3.1 r40.068 ROM" \
    "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"

python3 \
    "${repo_root}/test-data/commodore/amiga/programmable-hblank/tools/build.py" \
    --output "${dist_dir}"

if [[ ! -f "${dist_dir}/suite-v1.json" ]]; then
    echo "error: corpus build did not produce ${dist_dir}/suite-v1.json" >&2
    exit 1
fi

export EMU198X_AMIGA_PROGRAMMABLE_HBLANK_DIST="${dist_dir}"
export EMU198X_AMIGA_KICKSTART_204_ROM="${normalized_kickstart_204}"
export EMU198X_AMIGA_KICKSTART_31_A1200_ROM="${normalized_kickstart_31_a1200}"

cargo test --locked --release \
    -p runtime-commodore-amiga \
    --test amiga_programmable_hblank \
    -- --ignored --nocapture --test-threads=1

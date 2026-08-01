#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

usage() {
    cat <<'USAGE'
Usage: scripts/verify-amiga-test-kit-video-a1200.sh

Runs the explicit Amiga Test Kit v1.21 video-reference accuracy gate
for the PAL A1200 AGA profile.

Environment:
    EMU198X_AMIGA_TEST_KIT_V121_ADF       required Test Kit v1.21 ADF or ZIP
    EMU198X_AMIGA_KICKSTART_31_A1200_ROM  Kickstart 3.1 r40.068 A1200 ROM
    EMU198X_AMIGA_ROM_DIR                 fallback ROM directory
    CARGO_TARGET_DIR                      optional Cargo build directory

The input identities must match
test-data/amiga-test-kit-v1.21-a1200-aga-pal.sha256. Missing or mismatched
inputs fail the gate.
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

test_kit_source="${EMU198X_AMIGA_TEST_KIT_V121_ADF:-}"
if [[ -z "${test_kit_source}" || ! -f "${test_kit_source}" ]]; then
    echo "error: set EMU198X_AMIGA_TEST_KIT_V121_ADF to the Test Kit v1.21 ADF or ZIP" >&2
    exit 1
fi

if [[ -n "${EMU198X_AMIGA_KICKSTART_31_A1200_ROM:-}" ]]; then
    kickstart_source="${EMU198X_AMIGA_KICKSTART_31_A1200_ROM}"
else
    if [[ -n "${EMU198X_AMIGA_ROM_DIR:-}" ]]; then
        rom_dir="${EMU198X_AMIGA_ROM_DIR}"
    elif [[ -n "${HOME:-}" ]]; then
        rom_dir="${HOME}/.emu198x/roms/commodore-amiga"
    else
        echo "error: set EMU198X_AMIGA_KICKSTART_31_A1200_ROM or EMU198X_AMIGA_ROM_DIR" >&2
        exit 1
    fi
    kickstart_source="${rom_dir}/kick31a1200.rom"
fi
if [[ ! -f "${kickstart_source}" ]]; then
    echo "error: Kickstart 3.1 A1200 ROM not found at ${kickstart_source}" >&2
    exit 1
fi

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/emu198x-amiga-test-kit-video-a1200.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT

normalized_adf="${work_dir}/amiga-test-kit-v1.21.adf"
normalized_rom="${work_dir}/kickstart-3.1-a1200-r40.68.rom"

if unzip -Z1 "${test_kit_source}" >/dev/null 2>&1; then
    adf_members=()
    while IFS= read -r member; do
        adf_members+=("${member}")
    done < <(unzip -Z1 "${test_kit_source}" | awk 'tolower($0) ~ /\.adf$/')
    if [[ "${#adf_members[@]}" -ne 1 ]]; then
        echo "error: expected exactly one ADF member in ${test_kit_source}" >&2
        exit 1
    fi
    unzip -p "${test_kit_source}" "${adf_members[0]}" > "${normalized_adf}"
elif [[ "${test_kit_source##*.}" =~ ^[zZ][iI][pP]$ ]]; then
    echo "error: ${test_kit_source} is not a readable ZIP archive" >&2
    exit 1
else
    cp "${test_kit_source}" "${normalized_adf}"
fi

cp "${kickstart_source}" "${normalized_rom}"

if command -v sha256sum >/dev/null 2>&1; then
    (
        cd "${work_dir}"
        sha256sum -c "${repo_root}/test-data/amiga-test-kit-v1.21-a1200-aga-pal.sha256"
    )
else
    (
        cd "${work_dir}"
        shasum -a 256 -c "${repo_root}/test-data/amiga-test-kit-v1.21-a1200-aga-pal.sha256"
    )
fi

export EMU198X_AMIGA_TEST_KIT_V121_ADF="${normalized_adf}"
export EMU198X_AMIGA_KICKSTART_31_A1200_ROM="${normalized_rom}"

cargo test --locked --release \
    -p runtime-commodore-amiga \
    --test amiga_test_kit_video \
    amiga_test_kit_v121_a1200_aga_pal_matches_reference \
    -- --ignored --nocapture --test-threads=1

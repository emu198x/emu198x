#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

usage() {
    cat <<'USAGE'
Usage: scripts/verify-amiga-paula-audio.sh

Builds the emulator-neutral Paula-audio corpus and runs the explicit A500
routing, cadence, and paired-volume gate against the registered reference
package.

Environment:
    EMU198X_AMIGA_KICKSTART_13_ROM  Kickstart 1.3 r34.005 ROM
    EMU198X_AMIGA_ROM_DIR           fallback ROM directory
    CARGO_TARGET_DIR                optional Cargo build directory

When the explicit ROM path is absent, the script reads kick13.rom from
EMU198X_AMIGA_ROM_DIR, or from ~/.emu198x/roms/commodore-amiga by default.

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

kickstart_source="${EMU198X_AMIGA_KICKSTART_13_ROM:-}"
if [[ -z "${kickstart_source}" ]]; then
    if [[ -z "${rom_dir}" ]]; then
        echo "error: set EMU198X_AMIGA_KICKSTART_13_ROM or EMU198X_AMIGA_ROM_DIR" >&2
        exit 1
    fi
    kickstart_source="${rom_dir}/kick13.rom"
fi

if [[ ! -f "${kickstart_source}" ]]; then
    echo "error: Kickstart 1.3 ROM not found at ${kickstart_source}" >&2
    exit 1
fi

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/emu198x-amiga-paula-audio.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT

dist_dir="${work_dir}/dist"
firmware_dir="${work_dir}/firmware"
mkdir -p "${firmware_dir}"
normalized_kickstart="${firmware_dir}/kick13.rom"
cp "${kickstart_source}" "${normalized_kickstart}"

if command -v sha256sum >/dev/null 2>&1; then
    actual_sha256="$(sha256sum "${normalized_kickstart}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    actual_sha256="$(shasum -a 256 "${normalized_kickstart}" | awk '{print $1}')"
else
    echo "error: sha256sum or shasum is required to validate firmware" >&2
    exit 1
fi

actual_bytes="$(wc -c < "${normalized_kickstart}" | tr -d '[:space:]')"
if [[ "${actual_bytes}" != "262144" ]]; then
    echo "error: Kickstart 1.3 has ${actual_bytes} bytes; expected 262144" >&2
    exit 1
fi
expected_sha256="ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53"
if [[ "${actual_sha256}" != "${expected_sha256}" ]]; then
    echo "error: Kickstart 1.3 SHA-256 is ${actual_sha256}; expected ${expected_sha256}" >&2
    exit 1
fi

python3 \
    "${repo_root}/test-data/commodore/amiga/paula-audio/tools/build.py" \
    --output "${dist_dir}"

if [[ ! -f "${dist_dir}/suite-v1.json" ]]; then
    echo "error: corpus build did not produce ${dist_dir}/suite-v1.json" >&2
    exit 1
fi

export EMU198X_AMIGA_PAULA_AUDIO_DIST="${dist_dir}"
export EMU198X_AMIGA_KICKSTART_13_ROM="${normalized_kickstart}"

cargo test --locked --release \
    -p runtime-commodore-amiga \
    --test amiga_paula_audio \
    -- --ignored --nocapture --test-threads=1

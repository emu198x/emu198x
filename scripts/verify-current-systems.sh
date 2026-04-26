#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${EMU198X_VERIFY_OUT_DIR:-target/current-system-verification}"
report="${out_dir}/report.jsonl"
mode="all"

usage() {
    cat <<'USAGE'
Usage: scripts/verify-current-systems.sh [OPTIONS]

Runs the current-system verification gate for Spectrum, C64, NES, Amiga, and
Game Boy.

Options:
    --unit-only       run only in-repository unit/integration checks
    --local-only      run only local ROM/media smoke checks
    --help, -h        show this help

Environment:
    EMU198X_VERIFY_OUT_DIR       output directory [default: target/current-system-verification]
    EMU198X_REFERENCE_ROOT       local reference archive root
    EMU198X_SPECTRUM_48K_ROM     Spectrum 48K ROM
    EMU198X_C64_ROM_DIR          C64 ROM directory
    EMU198X_AMIGA_ROM_DIR        Amiga ROM directory
    EMU198X_NES_APU_TEST_ROOT    NES Blargg APU rom_singles directory
    EMU198X_GB_BLARGG_ROOT       Game Boy Blargg test ROM root
    EMU198X_GB_MOONEYE_ROOT      Game Boy mooneye-gb test ROM root

Missing local ROM/media assets are reported as skipped, not failed.
USAGE
}

while (($# > 0)); do
    case "$1" in
        --unit-only)
            mode="unit"
            ;;
        --local-only)
            mode="local"
            ;;
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
    shift
done

mkdir -p "${out_dir}"
: > "${report}"

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/	/\\t/g'
}

record() {
    local name="$1"
    local status="$2"
    local log="$3"
    local detail="$4"
    printf '{"name":"%s","status":"%s","log":"%s","detail":"%s"}\n' \
        "$(json_escape "${name}")" \
        "$(json_escape "${status}")" \
        "$(json_escape "${log}")" \
        "$(json_escape "${detail}")" >> "${report}"
}

run_step() {
    local name="$1"
    shift
    local log="${out_dir}/$(printf '%s' "${name}" | tr -cs 'A-Za-z0-9._-' '_').log"

    printf '== %s ==\n' "${name}"
    if "$@" > "${log}" 2>&1; then
        record "${name}" "pass" "${log}" ""
        printf 'PASS %s\n' "${name}"
    else
        record "${name}" "fail" "${log}" "command failed"
        printf 'FAIL %s (see %s)\n' "${name}" "${log}" >&2
        return 1
    fi
}

run_step_expect_log() {
    local name="$1"
    local needle="$2"
    shift 2
    local log="${out_dir}/$(printf '%s' "${name}" | tr -cs 'A-Za-z0-9._-' '_').log"

    printf '== %s ==\n' "${name}"
    if "$@" > "${log}" 2>&1 && grep -q "${needle}" "${log}"; then
        record "${name}" "pass" "${log}" "matched ${needle}"
        printf 'PASS %s\n' "${name}"
    else
        record "${name}" "fail" "${log}" "missing ${needle}"
        printf 'FAIL %s (see %s)\n' "${name}" "${log}" >&2
        return 1
    fi
}

skip_step() {
    local name="$1"
    local detail="$2"
    printf 'SKIP %s: %s\n' "${name}" "${detail}"
    record "${name}" "skip" "" "${detail}"
}

first_existing_dir() {
    for path in "$@"; do
        if [[ -d "${path}" ]]; then
            printf '%s\n' "${path}"
            return 0
        fi
    done
    return 1
}

first_existing_file() {
    for path in "$@"; do
        if [[ -f "${path}" ]]; then
            printf '%s\n' "${path}"
            return 0
        fi
    done
    return 1
}

write_boot_script() {
    local path="$1"
    local max_frames="$2"
    cat > "${path}" <<JSON
[
  {"action":"wait_for_query_bool","path":"boot.detected","value":true,"max_frames":${max_frames}},
  {"action":"query","path":"boot.detected"},
  {"action":"query","path":"boot.reason"}
]
JSON
}

reference_root="${EMU198X_REFERENCE_ROOT:-/Users/stevehill/Projects/Emu198x-docs-archive-2026-04-19/Reference}"
script_dir="${out_dir}/scripts"
mkdir -p "${script_dir}"

if [[ "${mode}" != "local" ]]; then
    run_step "current-script-runner-tests" \
        cargo test \
            -p emu198x-script-spectrum \
            -p emu198x-script-c64 \
            -p emu198x-script-nes \
            -p emu198x-script-amiga \
            -p emu198x-script-game-boy

    run_step "current-runtime-lib-tests" \
        cargo test \
            -p runtime-sinclair-zx-spectrum \
            -p runtime-commodore-c64 \
            -p runtime-nintendo-nes \
            -p runtime-commodore-amiga \
            -p runtime-nintendo-game-boy \
            --lib
fi

if [[ "${mode}" != "unit" ]]; then
    spectrum_rom="${EMU198X_SPECTRUM_48K_ROM:-${HOME}/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom}"
    write_boot_script "${script_dir}/spectrum-boot.json" 250
    if [[ -f "${spectrum_rom}" ]]; then
        run_step_expect_log "spectrum-48k-boot" '"value":true' \
            cargo run -q -p emu198x-script-spectrum -- \
                --rom "${spectrum_rom}" \
                --script "${script_dir}/spectrum-boot.json"
    else
        skip_step "spectrum-48k-boot" "missing ROM at ${spectrum_rom}"
    fi

    c64_rom_dir="${EMU198X_C64_ROM_DIR:-${HOME}/.emu198x/roms/commodore-c64}"
    write_boot_script "${script_dir}/c64-boot.json" 220
    if [[ -d "${c64_rom_dir}" ]]; then
        run_step_expect_log "c64-pal-boot" '"value":true' \
            cargo run -q -p emu198x-script-c64 -- \
                --rom-dir "${c64_rom_dir}" \
                --script "${script_dir}/c64-boot.json"
    else
        skip_step "c64-pal-boot" "missing ROM directory at ${c64_rom_dir}"
    fi

    amiga_rom_dir="${EMU198X_AMIGA_ROM_DIR:-${HOME}/.emu198x/roms/commodore-amiga}"
    write_boot_script "${script_dir}/amiga-a500-boot.json" 450
    if [[ -d "${amiga_rom_dir}" ]]; then
        run_step_expect_log "amiga-a500-boot" '"value":true' \
            cargo run -q -p emu198x-script-amiga -- \
                --rom-dir "${amiga_rom_dir}" \
                --model a500 \
                --script "${script_dir}/amiga-a500-boot.json"
    else
        skip_step "amiga-a500-boot" "missing ROM directory at ${amiga_rom_dir}"
    fi

    nes_apu_root="${EMU198X_NES_APU_TEST_ROOT:-}"
    if [[ -z "${nes_apu_root}" ]]; then
        nes_apu_root="$(first_existing_dir \
            "${reference_root}/nintendo/nes/test-suites/apu_test/rom_singles" \
            "${reference_root}/nintendo/nes/apu-tests/nes-test-roms-master/blargg_apu_2005.07.30" \
            || true)"
    fi
    if [[ -n "${nes_apu_root}" && -d "${nes_apu_root}" ]]; then
        run_step "nes-blargg-apu" \
            cargo run -q -p emu198x-script-nes -- \
                --smoke-root "${nes_apu_root}" \
                --frames 1200 \
                --assert-blargg \
                --smoke-report "${out_dir}/nes-blargg-apu.json"
    else
        skip_step "nes-blargg-apu" "missing NES APU test root; set EMU198X_NES_APU_TEST_ROOT"
    fi

    gb_blargg_root="${EMU198X_GB_BLARGG_ROOT:-}"
    if [[ -z "${gb_blargg_root}" ]]; then
        gb_blargg_root="$(first_existing_dir \
            "${reference_root}/nintendo/game-boy/gb-test-roms-master" \
            "${repo_root}/tmp/gb-test-roms-master" \
            "${repo_root}/tmp/mooneye-test-suite/../gb-test-roms-master" \
            "/Users/stevehill/Projects/Emu198x-Zig/gb-test-roms-master" \
            || true)"
    fi
    if [[ -n "${gb_blargg_root}" && -d "${gb_blargg_root}" ]]; then
        run_step "game-boy-blargg-cpu-instrs" \
            env EMU198X_GB_BLARGG_ROOT="${gb_blargg_root}" \
            cargo test -p runtime-nintendo-game-boy --test phase2_verification \
                blargg_cpu_instrs_passes_all_11_subtests -- --ignored --exact
    else
        skip_step "game-boy-blargg-cpu-instrs" "missing Game Boy Blargg root; set EMU198X_GB_BLARGG_ROOT"
    fi

    gb_mooneye_root="${EMU198X_GB_MOONEYE_ROOT:-}"
    if [[ -z "${gb_mooneye_root}" ]]; then
        gb_mooneye_root="$(first_existing_dir \
            "${repo_root}/tmp/mooneye-test-suite" \
            "${reference_root}/nintendo/game-boy/mooneye-test-suite" \
            || true)"
    fi
    if [[ -n "${gb_mooneye_root}" && -d "${gb_mooneye_root}" ]]; then
        run_step "game-boy-mooneye-gate" \
            env EMU198X_GB_MOONEYE_ROOT="${gb_mooneye_root}" \
            cargo test -p runtime-nintendo-game-boy --test phase2_verification \
                mooneye_acceptance_gate_set_passes -- --ignored --exact
    else
        skip_step "game-boy-mooneye-gate" "missing mooneye root; set EMU198X_GB_MOONEYE_ROOT"
    fi
fi

printf '\nReport: %s\n' "${report}"
if grep -q '"status":"fail"' "${report}"; then
    exit 1
fi

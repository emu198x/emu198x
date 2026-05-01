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

Runs the current-system verification gate for Spectrum, C64, NES, Amiga,
Game Boy, and Dragon.

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
    EMU198X_DRAGON32_ROM         Dragon 32 BASIC ROM or zip archive
    EMU198X_DRAGON_TEXTSTAR_CAS  Dragon Textstar CAS or zip archive
    EMU198X_DRAGON_CLOADM_CAS    Dragon machine-code CAS or zip archive
    EMU198X_DRAGON_AUDIO_CAS     Dragon CAS expected to produce non-silent audio
    EMU198X_DRAGON_JOYSTICK_CAS  Dragon CAS used for scripted joystick smoke
                                  and analogue comparator sweep smoke
    EMU198X_DRAGON_JOYSTICK_GAME_CAS
                                  Optional Dragon game CAS used for longer
                                  joystick-vs-idle smoke
    EMU198X_XROAR_BIN            patched XRoar binary for explicit optional
                                  Dragon reference

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

run_step_expect_file() {
    local name="$1"
    local file="$2"
    local needle="$3"
    shift 3
    local log="${out_dir}/$(printf '%s' "${name}" | tr -cs 'A-Za-z0-9._-' '_').log"

    printf '== %s ==\n' "${name}"
    rm -f "${file}"
    if "$@" > "${log}" 2>&1 && [[ -f "${file}" ]] && grep -q "${needle}" "${file}"; then
        record "${name}" "pass" "${log}" "matched ${needle} in ${file}"
        printf 'PASS %s\n' "${name}"
    else
        record "${name}" "fail" "${log}" "missing ${needle} in ${file}"
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

run_dragon_audio_smoke() {
    local rom="$1"
    local cas="$2"
    local smoke_report="$3"
    local artifact_root="$4"
    local audio_dir
    local screen_dir

    mkdir -p "${artifact_root}"
    audio_dir="$(mktemp -d "${artifact_root}/dragon-backgammon-audio.XXXXXX")"
    screen_dir="$(mktemp -d "${artifact_root}/dragon-backgammon-screens.XXXXXX")"
    cargo run -q -p emu198x-script-dragon -- \
        --rom "${rom}" \
        --smoke-root "${cas}" \
        --smoke-run-limit 1 \
        --smoke-report "${smoke_report}" \
        --smoke-audio-dir "${audio_dir}" \
        --smoke-screenshot-dir "${screen_dir}"

    python3 - "${smoke_report}" <<'PY'
import json
import struct
import sys
import wave

report_path = sys.argv[1]
with open(report_path, "r", encoding="utf-8") as handle:
    report = json.load(handle)

try:
    runtime = report["rows"][0]["runtime"]
    wav_path = runtime["start_audio"]
except (KeyError, IndexError, TypeError) as exc:
    raise SystemExit(f"missing runtime start_audio in {report_path}: {exc}") from exc

with wave.open(wav_path, "rb") as wav:
    if wav.getnchannels() != 1 or wav.getframerate() != 48_000:
        raise SystemExit(
            f"unexpected WAV format in {wav_path}: {wav.getnchannels()} ch {wav.getframerate()} Hz"
        )
    data = wav.readframes(wav.getnframes())

samples = struct.unpack("<" + "h" * (len(data) // 2), data) if data else ()
nonzero = sum(1 for sample in samples if sample != 0)
peak = max((abs(sample) for sample in samples), default=0)
if nonzero == 0 or peak == 0:
    raise SystemExit(f"silent Dragon audio capture: {wav_path}")

print(f"non-silent Dragon audio: {wav_path} nonzero={nonzero} peak={peak}")
PY
}

run_dragon_joystick_game_smoke() {
    local rom="$1"
    local cas="$2"
    local idle_report="$3"
    local input_report="$4"
    local artifact_root="$5"
    local idle_screen_dir
    local input_screen_dir

    mkdir -p "${artifact_root}"
    idle_screen_dir="$(mktemp -d "${artifact_root}/dragon-joystick-game-idle.XXXXXX")"
    input_screen_dir="$(mktemp -d "${artifact_root}/dragon-joystick-game-input.XXXXXX")"

    cargo run -q -p emu198x-script-dragon -- \
        --rom "${rom}" \
        --smoke-root "${cas}" \
        --smoke-run-limit 1 \
        --smoke-report "${idle_report}" \
        --smoke-screenshot-dir "${idle_screen_dir}" \
        --smoke-idle-after-start 492

    cargo run -q -p emu198x-script-dragon -- \
        --rom "${rom}" \
        --smoke-root "${cas}" \
        --smoke-run-limit 1 \
        --smoke-report "${input_report}" \
        --smoke-screenshot-dir "${input_screen_dir}" \
        --smoke-joystick 2,up,492

    python3 - "${idle_report}" "${input_report}" <<'PY'
import json
import sys
from pathlib import Path

idle_report, input_report = sys.argv[1:3]

def runtime(path):
    with open(path, "r", encoding="utf-8") as handle:
        report = json.load(handle)
    try:
        return report["rows"][0]["runtime"]
    except (KeyError, IndexError, TypeError) as exc:
        raise SystemExit(f"missing runtime in {path}: {exc}") from exc

idle = runtime(idle_report)
scripted = runtime(input_report)
if scripted.get("joystick_visible_change") is not True:
    raise SystemExit("scripted joystick input did not visibly change the game")

idle_path = Path(idle.get("idle_screenshot", ""))
input_path = Path(scripted.get("joystick_screenshot", ""))
if not idle_path.is_file() or not input_path.is_file():
    raise SystemExit(f"missing idle/input screenshots: {idle_path} {input_path}")

if idle_path.read_bytes() == input_path.read_bytes():
    raise SystemExit("scripted joystick screenshot matches no-input idle baseline")

print(f"Dragon joystick game input differs from idle baseline: {input_path}")
PY
}

run_dragon_joystick_axis_sweep_smoke() {
    local rom="$1"
    local cas="$2"
    local smoke_report="$3"
    local artifact_root="$4"
    local screen_dir

    mkdir -p "${artifact_root}"
    screen_dir="$(mktemp -d "${artifact_root}/dragon-joystick-axis-sweep.XXXXXX")"
    cargo run -q -p emu198x-script-dragon -- \
        --rom "${rom}" \
        --smoke-root "${cas}" \
        --smoke-run-limit 1 \
        --smoke-report "${smoke_report}" \
        --smoke-screenshot-dir "${screen_dir}" \
        --smoke-joystick-axis-sweep 1,x,-1.0,1.0,5,120 \
        --smoke-joystick-axis-sweep 1,y,-1.0,1.0,5,120 \
        --smoke-idle-after-start 120

    python3 - "${smoke_report}" <<'PY'
import json
import sys

report_path = sys.argv[1]
with open(report_path, "r", encoding="utf-8") as handle:
    report = json.load(handle)

try:
    runtime = report["rows"][0]["runtime"]
except (KeyError, IndexError, TypeError) as exc:
    raise SystemExit(f"missing runtime in {report_path}: {exc}") from exc

if runtime.get("classification") != "started-text-drawing":
    raise SystemExit(f"unexpected joystick fixture classification: {runtime.get('classification')}")
if runtime.get("start_command") != "RUN":
    raise SystemExit(f"unexpected joystick fixture start command: {runtime.get('start_command')}")
if runtime.get("idle_visible_change") is not False:
    raise SystemExit("joystick fixture did not reach a stable idle baseline")
if runtime.get("joystick_visible_change") is not True:
    raise SystemExit("analogue joystick sweep did not visibly affect fixture output")

results = runtime.get("joystick_axis_sweep_results", [])
for axis in ("x", "y"):
    axis_results = [result for result in results if result.get("port") == 1 and result.get("axis") == axis]
    if len(axis_results) != 5:
        raise SystemExit(f"expected 5 sweep results for axis {axis}, got {len(axis_results)}")
    if not any(result.get("visible_change") is True for result in axis_results):
        raise SystemExit(f"axis {axis} sweep produced no visible response")

print(f"Dragon analogue joystick sweep passed: {report_path}")
PY
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
            -p emu198x-script-game-boy \
            -p emu198x-script-dragon

    run_step "current-runtime-lib-tests" \
        cargo test \
            -p runtime-sinclair-zx-spectrum \
            -p runtime-commodore-c64 \
            -p runtime-nintendo-nes \
            -p runtime-commodore-amiga \
            -p runtime-nintendo-game-boy \
            -p runtime-dragon \
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

    dragon_rom="${EMU198X_DRAGON32_ROM:-}"
    if [[ -z "${dragon_rom}" ]]; then
        dragon_rom="$(first_existing_file \
            "${HOME}/.emu198x/roms/dragon/dragon32.rom" \
            "${reference_root}/dragon/Dragon/Firmware/Dragon Data Dragon 32 BIOS (1982)(Dragon Data).zip" \
            || true)"
    fi

    dragon_textstar_cas="${EMU198X_DRAGON_TEXTSTAR_CAS:-}"
    if [[ -z "${dragon_textstar_cas}" ]]; then
        dragon_textstar_cas="$(first_existing_file \
            "${reference_root}/dragon/Dragon/Applications/[CAS]/Textstar (1982)(Personal Software Services).zip" \
            || true)"
    fi

    dragon_cloadm_cas="${EMU198X_DRAGON_CLOADM_CAS:-}"
    if [[ -z "${dragon_cloadm_cas}" ]]; then
        dragon_cloadm_cas="$(first_existing_file \
            "${reference_root}/dragon/Dragon/Games/[CAS]/Color Invaders (1982)(Microdeal).zip" \
            "${reference_root}/dragon/Dragon/Games/[CAS]/Color Invaders (1982)(Microdeal)[a].zip" \
            || true)"
    fi

    dragon_audio_cas="${EMU198X_DRAGON_AUDIO_CAS:-}"
    if [[ -z "${dragon_audio_cas}" ]]; then
        dragon_audio_cas="$(first_existing_file \
            "${reference_root}/dragon/Dragon/Games/[CAS]/Backgammon (1983)(Oasis).zip" \
            || true)"
    fi

    dragon_joystick_cas="${EMU198X_DRAGON_JOYSTICK_CAS:-}"
    if [[ -z "${dragon_joystick_cas}" ]]; then
        dragon_joystick_cas="$(first_existing_file \
            "${reference_root}/dragon/Dragon/Applications/[CAS]/Joystick Test (198x)(-).zip" \
            || true)"
    fi

    dragon_joystick_game_cas="${EMU198X_DRAGON_JOYSTICK_GAME_CAS:-}"

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" ]]; then
        run_step "dragon-real-rom-runtime" \
            env EMU198X_DRAGON32_ROM="${dragon_rom}" \
            cargo test -p runtime-dragon --test golden_basic \
                dragon32_real_rom_reaches_basic_prompt_and_captures_frame -- --exact
    else
        skip_step "dragon-real-rom-runtime" "missing Dragon 32 ROM; set EMU198X_DRAGON32_ROM"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_textstar_cas}" && -f "${dragon_textstar_cas}" ]]; then
        run_step_expect_file "dragon-textstar-cload-run" \
            "${out_dir}/dragon-textstar-smoke.json" \
            '"classification": "started-text-drawing"' \
            cargo run -q -p emu198x-script-dragon -- \
                --rom "${dragon_rom}" \
                --smoke-root "${dragon_textstar_cas}" \
                --smoke-run-limit 1 \
                --smoke-report "${out_dir}/dragon-textstar-smoke.json" \
                --smoke-screenshot-dir "${out_dir}/dragon-textstar-screens"
    else
        skip_step "dragon-textstar-cload-run" "missing Dragon ROM or Textstar CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_TEXTSTAR_CAS"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_cloadm_cas}" && -f "${dragon_cloadm_cas}" ]]; then
        run_step_expect_file "dragon-cloadm-exec" \
            "${out_dir}/dragon-cloadm-smoke.json" \
            '"start_command": "EXEC"' \
            cargo run -q -p emu198x-script-dragon -- \
                --rom "${dragon_rom}" \
                --smoke-root "${dragon_cloadm_cas}" \
                --smoke-run-limit 1 \
                --smoke-report "${out_dir}/dragon-cloadm-smoke.json" \
                --smoke-screenshot-dir "${out_dir}/dragon-cloadm-screens"
    else
        skip_step "dragon-cloadm-exec" "missing Dragon ROM or machine-code CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_CLOADM_CAS"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_audio_cas}" && -f "${dragon_audio_cas}" ]]; then
        run_step "dragon-backgammon-audio" \
            run_dragon_audio_smoke \
                "${dragon_rom}" \
                "${dragon_audio_cas}" \
                "${out_dir}/dragon-backgammon-audio-smoke.json" \
                "${out_dir}"
    else
        skip_step "dragon-backgammon-audio" "missing Dragon ROM or audio CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_AUDIO_CAS"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_joystick_cas}" && -f "${dragon_joystick_cas}" ]]; then
        run_step_expect_file "dragon-joystick-scripted-input" \
            "${out_dir}/dragon-joystick-smoke.json" \
            '"joystick_visible_change": true' \
            cargo run -q -p emu198x-script-dragon -- \
                --rom "${dragon_rom}" \
                --smoke-root "${dragon_joystick_cas}" \
                --smoke-run-limit 1 \
                --smoke-report "${out_dir}/dragon-joystick-smoke.json" \
                --smoke-screenshot-dir "${out_dir}/dragon-joystick-screens" \
                --smoke-joystick 1,right,300
    else
        skip_step "dragon-joystick-scripted-input" "missing Dragon ROM or joystick CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_JOYSTICK_CAS"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_joystick_cas}" && -f "${dragon_joystick_cas}" ]]; then
        run_step "dragon-joystick-axis-sweep" \
            run_dragon_joystick_axis_sweep_smoke \
                "${dragon_rom}" \
                "${dragon_joystick_cas}" \
                "${out_dir}/dragon-joystick-axis-sweep-smoke.json" \
                "${out_dir}"
    else
        skip_step "dragon-joystick-axis-sweep" "missing Dragon ROM or joystick CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_JOYSTICK_CAS"
    fi

    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_joystick_game_cas}" && -f "${dragon_joystick_game_cas}" ]]; then
        run_step "dragon-joystick-game-input" \
            run_dragon_joystick_game_smoke \
                "${dragon_rom}" \
                "${dragon_joystick_game_cas}" \
                "${out_dir}/dragon-joystick-game-idle-smoke.json" \
                "${out_dir}/dragon-joystick-game-input-smoke.json" \
                "${out_dir}"
    else
        skip_step "dragon-joystick-game-input" "missing Dragon ROM or explicit joystick game CAS; set EMU198X_DRAGON32_ROM and EMU198X_DRAGON_JOYSTICK_GAME_CAS for this longer smoke"
    fi

    xroar_bin="${EMU198X_XROAR_BIN:-}"
    if [[ -n "${dragon_rom}" && -f "${dragon_rom}" && -n "${dragon_textstar_cas}" && -f "${dragon_textstar_cas}" && -n "${xroar_bin}" && -x "${xroar_bin}" ]]; then
        run_step_expect_file "dragon-xroar-textstar-reference" \
            "${out_dir}/dragon-xroar-textstar-smoke.json" \
            '"differing_pixels": 0' \
            cargo run -q -p emu198x-script-dragon -- \
                --rom "${dragon_rom}" \
                --smoke-root "${dragon_textstar_cas}" \
                --smoke-run-limit 1 \
                --smoke-report "${out_dir}/dragon-xroar-textstar-smoke.json" \
                --smoke-screenshot-dir "${out_dir}/dragon-xroar-textstar-screens" \
                --smoke-screenshot-format xroar-zoomed \
                --xroar-bin "${xroar_bin}" \
                --xroar-reference-dir "${out_dir}/dragon-xroar-textstar-reference"
    else
        skip_step "dragon-xroar-textstar-reference" "missing Dragon ROM, Textstar CAS, or patched XRoar; set EMU198X_DRAGON32_ROM, EMU198X_DRAGON_TEXTSTAR_CAS, and EMU198X_XROAR_BIN"
    fi
fi

printf '\nReport: %s\n' "${report}"
if grep -q '"status":"fail"' "${report}"; then
    exit 1
fi

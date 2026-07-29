#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'USAGE'
Usage: capture.sh PROFILE CASE COPPERLINE SUITE_DIR FIRMWARE OUTPUT_DIR OPERATOR

PROFILE is ecs or aga. CASE is a suite case id. COPPERLINE is the exact
eec5806778dab8b60f3b05fa7ab2428e4e18b073 release binary. SUITE_DIR contains
suite-v1.json and its ADFs. FIRMWARE is the external Kickstart image.
OUTPUT_DIR must not already exist. OPERATOR identifies who supervised the
capture.
USAGE
}

if (($# != 7)); then
    usage >&2
    exit 2
fi

profile="$1"
case_id="$2"
copperline="$3"
suite_dir="$4"
firmware="$5"
output_dir="$6"
operator="$7"

case "${profile}" in
    ecs|aga) ;;
    *)
        echo "error: PROFILE must be ecs or aga" >&2
        exit 2
        ;;
esac

if [[ ! "${case_id}" =~ ^[a-z][a-z0-9-]*$ ]]; then
    echo "error: invalid case id: ${case_id}" >&2
    exit 2
fi
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
copperline="$(
    python3 -c \
        'import sys; from pathlib import Path; print(Path(sys.argv[1]).resolve(strict=True))' \
        "${copperline}"
)"

export RUST_LOG=info
export COPPERLINE_SHOT_RAW=1
export COPPERLINE_HCENTER=0
export COPPERLINE_OVERSCAN=full
export COPPERLINE_PIXEL_ASPECT=square
export COPPERLINE_DEINTERLACE=0
export COPPERLINE_PHOSPHOR=0
export COPPERLINE_THREADED_RENDER=0
export COPPERLINE_DBG_BREAK=300aa
export COPPERLINE_DBG_DUMP=2ff00:48
export COPPERLINE_DBG_FC=2ff0a
export COPPERLINE_DBG_AFTER=7.8
export COPPERLINE_DBG_UNTIL=8.2
export COPPERLINE_DBG_MAXHITS=1
export COPPERLINE_DUMP_RENDER_META=1

python3 "${script_dir}/capture_manifest.py" prepare \
    "${profile}" \
    "${case_id}" \
    "${copperline}" \
    "${suite_dir}" \
    "${firmware}" \
    "${output_dir}" \
    "${operator}"

(
    cd "${output_dir}"
    "${copperline}" \
        --config capture.toml \
        --noaudio \
        --dump-frames frames \
        --dump-start 8 \
        --dump-count 3
) 2>&1 | tee "${output_dir}/run.log"

python3 "${script_dir}/capture_manifest.py" verify \
    "${copperline}" \
    "${output_dir}"

#!/bin/sh
set -eu
export LC_ALL=C

if [ "$#" -ne 6 ]; then
    echo "usage: capture.sh CASE FS_UAE_BINARY TEST_KIT_ADF FIRMWARE OUTPUT_ROOT OPERATOR" >&2
    exit 64
fi

case_id="$1"
binary="$2"
test_kit_adf="$3"
firmware="$4"
output_root="$5"
operator="$6"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
binary="$(python3 "${script_dir}/capture_manifest.py" resolve "${binary}")"
test_kit_adf="$(python3 "${script_dir}/capture_manifest.py" resolve "${test_kit_adf}")"
firmware="$(python3 "${script_dir}/capture_manifest.py" resolve "${firmware}")"
output_root="$(python3 "${script_dir}/capture_manifest.py" resolve "${output_root}")"
pid=""

cleanup() {
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
        kill -TERM "${pid}" 2>/dev/null || true
        wait "${pid}" 2>/dev/null || true
    fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if [ "${case_id}" = "all" ]; then
    for registered_case in \
        gradients \
        static-checkerboard \
        alternating-checkerboard \
        ebu-bars \
        dots \
        crosshatch
    do
        "${script_dir}/capture.sh" \
            "${registered_case}" \
            "${binary}" \
            "${test_kit_adf}" \
            "${firmware}" \
            "${output_root}" \
            "${operator}"
    done
    exit 0
fi

case "${case_id}" in
    gradients|static-checkerboard|alternating-checkerboard|ebu-bars|dots|crosshatch)
        ;;
    *)
        echo "unknown Test Kit case: ${case_id}" >&2
        exit 64
        ;;
esac

expected_binary_sha256="5c3d9e35d100445a5603c5f86a19cc431a7363828053d4ede7d260c2c5d6899f"
expected_adf_sha256="abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28"
expected_firmware_sha256="6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"
run_dir="${output_root}/${case_id}"

if [ ! -x "${binary}" ]; then
    echo "FS-UAE binary is not executable: ${binary}" >&2
    exit 66
fi
if [ ! -f "${test_kit_adf}" ]; then
    echo "Test Kit ADF is absent: ${test_kit_adf}" >&2
    exit 66
fi
if [ ! -f "${firmware}" ]; then
    echo "firmware is absent: ${firmware}" >&2
    exit 66
fi
if [ "$(shasum -a 256 "${binary}" | awk '{print $1}')" != "${expected_binary_sha256}" ]; then
    echo "FS-UAE binary hash mismatch" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${test_kit_adf}" | awk '{print $1}')" != "${expected_adf_sha256}" ]; then
    echo "Test Kit ADF hash mismatch" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${firmware}" | awk '{print $1}')" != "${expected_firmware_sha256}" ]; then
    echo "A1200 firmware hash mismatch" >&2
    exit 65
fi
if [ -e "${run_dir}" ]; then
    echo "output already exists: ${run_dir}" >&2
    exit 73
fi

runtime_portable="$(
    python3 "${script_dir}/capture_manifest.py" verify-portable "${binary}"
)"

mkdir -p \
    "${run_dir}/base/Kickstarts" \
    "${run_dir}/base/Floppies" \
    "${run_dir}/base/Hard Drives" \
    "${run_dir}/base/CD-ROMs" \
    "${run_dir}/base/Save States" \
    "${run_dir}/capture" \
    "${run_dir}/inputs"

install -m 0444 "${test_kit_adf}" "${run_dir}/inputs/AmigaTestKit.adf"
staged_adf="${run_dir}/inputs/AmigaTestKit.adf"

python3 "${script_dir}/capture_manifest.py" config \
    "${script_dir}/config.uae.in" \
    "${run_dir}/config.uae" \
    "${case_id}" \
    "${run_dir}" \
    "${firmware}" \
    "${staged_adf}"

captured_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%S+00:00')"
host="$(python3 "${script_dir}/capture_manifest.py" host)"

shasum -a 256 \
    "${firmware}" \
    "${staged_adf}" \
    "${run_dir}/config.uae" \
    "${binary}" \
    "${script_dir}/capture.sh" \
    "${script_dir}/capture_manifest.py" \
    "${script_dir}/config.uae.in" \
    "${script_dir}/Portable.ini" \
    "${script_dir}/fs-uae-5.0.7-test-kit-video-capture.patch" \
    "${runtime_portable}" \
    > "${run_dir}/inputs-before.sha256"

env \
    FSEMU_QUIT_AFTER_N_FRAMES=1000 \
    FSEMU_CODEX_TESTKIT_CAPTURE_DIR="${run_dir}/capture" \
    FSEMU_CODEX_TESTKIT_CASE="${case_id}" \
    "${binary}" "${run_dir}/config.uae" > "${run_dir}/run.stdout" 2>&1 &
pid="$!"
echo "${pid}" > "${run_dir}/pid"

complete=0
attempt=0
while [ "${attempt}" -lt 240 ]; do
    if rg -q "^CODEX_TESTKIT complete case=${case_id} " "${run_dir}/run.stdout"; then
        complete=1
        break
    fi
    if ! kill -0 "${pid}" 2>/dev/null; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.25
done

if kill -0 "${pid}" 2>/dev/null; then
    kill -TERM "${pid}"
fi
set +e
wait "${pid}"
wait_status="$?"
set -e
pid=""
echo "${wait_status}" > "${run_dir}/frontend-wait-status"
case "${wait_status}" in
    0|143)
        ;;
    *)
        echo "unexpected FS-UAE frontend wait status: ${wait_status}" >&2
        exit 70
        ;;
esac

if [ "${complete}" -ne 1 ]; then
    echo "capture did not complete for ${case_id}" >&2
    exit 70
fi
if rg -q '^CODEX_TESTKIT error=' "${run_dir}/run.stdout"; then
    echo "capture hook reported an error for ${case_id}" >&2
    exit 70
fi

raw_count="$(find "${run_dir}/capture" -maxdepth 1 -type f -name '*.bgra' | wc -l | tr -d ' ')"
metadata_count="$(find "${run_dir}/capture" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d ' ')"
if [ "${raw_count}" -ne 3 ] || [ "${metadata_count}" -ne 3 ]; then
    echo "unexpected capture count for ${case_id}" >&2
    exit 70
fi

shasum -a 256 \
    "${firmware}" \
    "${staged_adf}" \
    "${run_dir}/config.uae" \
    "${binary}" \
    "${script_dir}/capture.sh" \
    "${script_dir}/capture_manifest.py" \
    "${script_dir}/config.uae.in" \
    "${script_dir}/Portable.ini" \
    "${script_dir}/fs-uae-5.0.7-test-kit-video-capture.patch" \
    "${runtime_portable}" \
    > "${run_dir}/inputs-after.sha256"
cmp "${run_dir}/inputs-before.sha256" "${run_dir}/inputs-after.sha256"

shasum -a 256 "${run_dir}"/capture/* > "${run_dir}/capture.sha256"

python3 "${script_dir}/capture_manifest.py" write \
    "${run_dir}" \
    "${case_id}" \
    "${binary}" \
    "${staged_adf}" \
    "${firmware}" \
    "${captured_at_utc}" \
    "${operator}" \
    "${host}"

echo "captured A1200 AGA Test Kit ${case_id} at ${run_dir}"

#!/bin/sh
set -eu

if [ "$#" -ne 7 ]; then
    echo "usage: capture.sh PROFILE CASE FS_UAE_BINARY SUITE_DIR FIRMWARE OUTPUT_ROOT OPERATOR" >&2
    exit 64
fi

profile="$1"
case_id="$2"
binary="$3"
suite_dir="$4"
rom="$5"
output_root="$6"
operator="$7"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
run_dir="${output_root}/${profile}/${case_id}"
expected_binary_sha256="81fdcc09bf36b6a275a9d39b27407e3484815b5713b411e16dbfe6024cf2899b"
expected_suite_sha256="f8f70818fb0a7454db283deb48b75858302bece28922f3b1f2dfab0d59503b24"

case_number="$(
    python3 "${script_dir}/capture_manifest.py" case-number \
        "${suite_dir}/suite-v1.json" "${case_id}"
)"

case "${profile}" in
    ecs)
        chipset="ecs"
        model="A500+"
        rtc="MSM6242B"
        cia_tod_bug="true"
        pcmcia="false"
        ide="none"
        ks_mirror="false"
        chipmem="2"
        cpu_type="68000"
        cpu_model="68000"
        cpu_multiplier="2"
        expected_rom_sha256="d0b70e8a1772614b897f92c33cb299bed3fc8e3de488fc12f67f97fc2486eb79"
        ;;
    aga)
        chipset="aga"
        model="A1200"
        rtc="none"
        cia_tod_bug="false"
        pcmcia="true"
        ide="a600/a1200"
        ks_mirror="true"
        chipmem="4"
        cpu_type="68ec020"
        cpu_model="68020"
        cpu_multiplier="4"
        expected_rom_sha256="6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"
        ;;
    *)
        echo "profile must be ecs or aga" >&2
        exit 64
        ;;
esac

if [ ! -x "${binary}" ]; then
    echo "FS-UAE binary is not executable: ${binary}" >&2
    exit 66
fi
if [ "$(shasum -a 256 "${binary}" | awk '{print $1}')" != "${expected_binary_sha256}" ]; then
    echo "FS-UAE binary hash mismatch" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${suite_dir}/suite-v1.json" | awk '{print $1}')" != "${expected_suite_sha256}" ]; then
    echo "suite manifest hash mismatch" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${rom}" | awk '{print $1}')" != "${expected_rom_sha256}" ]; then
    echo "firmware hash mismatch for ${profile}" >&2
    exit 65
fi

adf_source="${suite_dir}/${case_id}.adf"
payload_source="${suite_dir}/${case_id}.bin"
if [ ! -f "${adf_source}" ] || [ ! -f "${payload_source}" ]; then
    echo "missing suite artifact for ${case_id}" >&2
    exit 66
fi
if [ -e "${run_dir}" ]; then
    echo "output already exists: ${run_dir}" >&2
    exit 73
fi

mkdir -p \
    "${run_dir}/base/Kickstarts" \
    "${run_dir}/base/Floppies" \
    "${run_dir}/base/Hard Drives" \
    "${run_dir}/base/CD-ROMs" \
    "${run_dir}/capture" \
    "${run_dir}/inputs"

install -m 0444 "${suite_dir}/suite-v1.json" "${run_dir}/inputs/suite-v1.json"
install -m 0444 "${adf_source}" "${run_dir}/inputs/${case_id}.adf"
install -m 0444 "${payload_source}" "${run_dir}/inputs/${case_id}.bin"
adf="${run_dir}/inputs/${case_id}.adf"

sed \
    -e "s#@PROFILE@#${profile}#g" \
    -e "s#@CASE@#${case_id}#g" \
    -e "s#@RUN_DIR@#${run_dir}#g" \
    -e "s#@ROM@#${rom}#g" \
    -e "s#@ADF@#${adf}#g" \
    -e "s#@CHIPSET@#${chipset}#g" \
    -e "s#@MODEL@#${model}#g" \
    -e "s#@RTC@#${rtc}#g" \
    -e "s#@CIA_TOD_BUG@#${cia_tod_bug}#g" \
    -e "s#@PCMCIA@#${pcmcia}#g" \
    -e "s#@IDE@#${ide}#g" \
    -e "s#@KS_MIRROR@#${ks_mirror}#g" \
    -e "s#@CHIPMEM@#${chipmem}#g" \
    -e "s#@CPU_TYPE@#${cpu_type}#g" \
    -e "s#@CPU_MODEL@#${cpu_model}#g" \
    -e "s#@CPU_MULTIPLIER@#${cpu_multiplier}#g" \
    "${script_dir}/config.uae.in" > "${run_dir}/config.uae"

captured_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%S+00:00')"
host="$(python3 "${script_dir}/capture_manifest.py" host)"

shasum -a 256 \
    "${rom}" \
    "${adf}" \
    "${run_dir}/inputs/${case_id}.bin" \
    "${run_dir}/inputs/suite-v1.json" \
    "${run_dir}/config.uae" \
    "${binary}" \
    "${script_dir}/capture.sh" \
    "${script_dir}/capture_manifest.py" \
    "${script_dir}/config.uae.in" > "${run_dir}/inputs-before.sha256"

env \
    FSEMU_QUIT_AFTER_N_FRAMES=600 \
    FSEMU_CODEX_CAPTURE_DIR="${run_dir}/capture" \
    FSEMU_CODEX_CAPTURE_CASE_NUMBER="${case_number}" \
    FSEMU_CODEX_CAPTURE_MIN_FIELD_COUNTER=9 \
    "${binary}" "${run_dir}/config.uae" > "${run_dir}/run.stdout" 2>&1 &
pid="$!"
echo "${pid}" > "${run_dir}/pid"

complete=0
attempt=0
while [ "${attempt}" -lt 240 ]; do
    if rg -q '^CODEX_CAPTURE complete ' "${run_dir}/run.stdout"; then
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
wait "${pid}" || true

if [ "${complete}" -ne 1 ]; then
    echo "capture did not complete for ${profile}/${case_id}" >&2
    exit 70
fi
if rg -q '^CODEX_CAPTURE (error|discontinuity)' "${run_dir}/run.stdout"; then
    echo "capture hook reported an error for ${profile}/${case_id}" >&2
    exit 70
fi

raw_count="$(find "${run_dir}/capture" -maxdepth 1 -type f -name '*.bgra' | wc -l | tr -d ' ')"
metadata_count="$(find "${run_dir}/capture" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d ' ')"
if [ "${raw_count}" -ne 3 ] || [ "${metadata_count}" -ne 3 ]; then
    echo "unexpected capture count for ${profile}/${case_id}" >&2
    exit 70
fi

shasum -a 256 \
    "${rom}" \
    "${adf}" \
    "${run_dir}/inputs/${case_id}.bin" \
    "${run_dir}/inputs/suite-v1.json" \
    "${run_dir}/config.uae" \
    "${binary}" \
    "${script_dir}/capture.sh" \
    "${script_dir}/capture_manifest.py" \
    "${script_dir}/config.uae.in" > "${run_dir}/inputs-after.sha256"
cmp "${run_dir}/inputs-before.sha256" "${run_dir}/inputs-after.sha256"

shasum -a 256 "${run_dir}"/capture/* > "${run_dir}/capture.sha256"

python3 "${script_dir}/capture_manifest.py" write \
    "${run_dir}" \
    "${profile}" \
    "${case_id}" \
    "${binary}" \
    "${rom}" \
    "${captured_at_utc}" \
    "${operator}" \
    "${host}"

echo "captured ${profile}/${case_id} at ${run_dir}"

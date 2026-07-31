#!/bin/sh
set -eu

if [ "$#" -ne 6 ]; then
    echo "usage: capture.sh CASE_OR_ALL VAMIGA_SOURCE SUITE_DIR FIRMWARE OUTPUT_ROOT OPERATOR" >&2
    exit 64
fi

selector="$1"
vamiga_source="$2"
suite_dir="$3"
firmware="$4"
output_root="$5"
operator="$6"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
expected_revision="60fd1e6b69dcd77c9f44d1291bd37ec715362ab0"
expected_firmware_sha256="ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53"

if [ ! -d "${vamiga_source}/Core" ]; then
    echo "vAmiga source root is invalid: ${vamiga_source}" >&2
    exit 66
fi
if [ ! -f "${suite_dir}/suite-v1.json" ]; then
    echo "suite directory has no suite-v1.json: ${suite_dir}" >&2
    exit 66
fi
if [ ! -f "${firmware}" ]; then
    echo "firmware is missing: ${firmware}" >&2
    exit 66
fi
if [ -e "${output_root}" ]; then
    echo "output root already exists: ${output_root}" >&2
    exit 73
fi

revision="$(git -c core.fsmonitor=false -C "${vamiga_source}" rev-parse HEAD)"
if [ "${revision}" != "${expected_revision}" ]; then
    echo "vAmiga revision mismatch: ${revision}" >&2
    exit 65
fi
if [ -n "$(git -c core.fsmonitor=false -C "${vamiga_source}" status --porcelain)" ]; then
    echo "vAmiga source tree is not clean" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${firmware}" | awk '{print $1}')" != "${expected_firmware_sha256}" ]; then
    echo "firmware hash does not match Kickstart 1.3 revision 34.005" >&2
    exit 65
fi

all_cases="$(
    python3 "${script_dir}/capture_record.py" \
        list-cases "${suite_dir}/suite-v1.json"
)"
if [ "${selector}" = "all" ]; then
    selected_cases="${all_cases}"
else
    python3 "${script_dir}/capture_record.py" \
        require-case "${suite_dir}/suite-v1.json" "${selector}"
    selected_cases="${selector}"
fi

mkdir -p "${output_root}"
capture_build_temporary=0
if [ -n "${EMU198X_VAMIGA_PAULA_BUILD_DIR:-}" ]; then
    capture_build_dir="${EMU198X_VAMIGA_PAULA_BUILD_DIR}"
    mkdir -p "${capture_build_dir}"
else
    capture_build_root="$(mktemp -d "${TMPDIR:-/tmp}/emu198x-vamiga-paula.XXXXXX")"
    capture_build_dir="${capture_build_root}/build"
    capture_build_temporary=1
fi
cleanup()
{
    if [ "${capture_build_temporary}" -eq 1 ]; then
        rm -rf "${capture_build_root}"
    fi
}
trap cleanup EXIT HUP INT TERM

build_log="${output_root}/producer-build.log"
{
    cmake \
        -S "${script_dir}" \
        -B "${capture_build_dir}" \
        -DVAMIGA_SOURCE="${vamiga_source}" \
        -DCMAKE_BUILD_TYPE=Release
    cmake \
        --build "${capture_build_dir}" \
        --config Release \
        --target vamiga-paula-audio-capture \
        --parallel
} > "${build_log}" 2>&1

binary="${capture_build_dir}/vamiga-paula-audio-capture"
if [ ! -x "${binary}" ]; then
    echo "adapter build did not produce ${binary}" >&2
    exit 70
fi

build_record="${output_root}/producer-build.json"
python3 "${script_dir}/capture_record.py" \
    build-record \
    "${revision}" \
    "${binary}" \
    "${build_log}" \
    "${build_record}"

for case_id in ${selected_cases}; do
    python3 "${script_dir}/capture_record.py" \
        capture \
        "${case_id}" \
        "${binary}" \
        "${suite_dir}" \
        "${firmware}" \
        "${output_root}" \
        "${operator}" \
        "${revision}" \
        "${build_record}"
done

if [ "${selector}" = "all" ]; then
    python3 "${script_dir}/capture_record.py" \
        verify-suite "${suite_dir}/suite-v1.json" "${output_root}"
fi

if [ "$(git -c core.fsmonitor=false -C "${vamiga_source}" rev-parse HEAD)" != "${revision}" ]; then
    echo "vAmiga revision changed during capture" >&2
    exit 65
fi
if [ -n "$(git -c core.fsmonitor=false -C "${vamiga_source}" status --porcelain)" ]; then
    echo "vAmiga source tree changed during capture" >&2
    exit 65
fi

echo "captured ${selector} with vAmiga ${revision} at ${output_root}"

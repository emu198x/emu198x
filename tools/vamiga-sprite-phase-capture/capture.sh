#!/bin/sh
set -eu

if [ "$#" -ne 5 ]; then
    echo "usage: capture.sh VAMIGA_SOURCE SUITE_DIR FIRMWARE OUTPUT_ROOT OPERATOR" >&2
    exit 64
fi

vamiga_source="$1"
suite_dir="$2"
firmware="$3"
output_root="$4"
operator="$5"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
expected_revision="60fd1e6b69dcd77c9f44d1291bd37ec715362ab0"
expected_suite_sha256="1390ffb208e1829f2fe1c12f1aae90e7a1b1981cdcf8cb2426b1da8611b4301b"
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
if [ -z "${operator}" ]; then
    echo "operator must not be empty" >&2
    exit 64
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
if [ "$(shasum -a 256 "${suite_dir}/suite-v1.json" | awk '{print $1}')" != "${expected_suite_sha256}" ]; then
    echo "sprite-phase suite manifest hash mismatch" >&2
    exit 65
fi
if [ "$(shasum -a 256 "${firmware}" | awk '{print $1}')" != "${expected_firmware_sha256}" ]; then
    echo "firmware hash does not match Kickstart 1.3 revision 34.005" >&2
    exit 65
fi

mkdir -p "${output_root}"
capture_build_temporary=0
if [ -n "${EMU198X_VAMIGA_SPRITE_BUILD_DIR:-}" ]; then
    capture_build_dir="${EMU198X_VAMIGA_SPRITE_BUILD_DIR}"
    mkdir -p "${capture_build_dir}"
else
    capture_build_root="$(mktemp -d "${TMPDIR:-/tmp}/emu198x-vamiga-sprite.XXXXXX")"
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
        --target vamiga-sprite-phase-capture \
        --parallel
} > "${build_log}" 2>&1

binary="${capture_build_dir}/vamiga-sprite-phase-capture"
if [ ! -x "${binary}" ]; then
    echo "adapter build did not produce ${binary}" >&2
    exit 70
fi

python3 "${script_dir}/capture_record.py" capture \
    "${binary}" \
    "${suite_dir}" \
    "${firmware}" \
    "${output_root}" \
    "${operator}" \
    "${revision}" \
    "${build_log}"

if [ "$(git -c core.fsmonitor=false -C "${vamiga_source}" rev-parse HEAD)" != "${revision}" ]; then
    echo "vAmiga revision changed during capture" >&2
    exit 65
fi
if [ -n "$(git -c core.fsmonitor=false -C "${vamiga_source}" status --porcelain)" ]; then
    echo "vAmiga source tree changed during capture" >&2
    exit 65
fi

echo "captured fixed-lores-sprite with vAmiga ${revision} at ${output_root}"

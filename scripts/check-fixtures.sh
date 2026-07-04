#!/usr/bin/env bash

# Reports which external accuracy corpora are reachable, by checking each
# corpus's env var and target path. Used two ways:
#   - locally, to see what the ignored CPU-conformance tests can run against;
#   - in CI, as a preflight before the nightly-accuracy workflow's test steps.
#
# The corpus contract (env var per corpus, sources, the mirror store) is
# documented in test-data/accuracy-corpora.md — keep this list in sync with it.
#
# Exit codes: 0 if every corpus is present (or in report mode); 1 in --strict
# mode when any corpus is missing.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

strict=false
[ "${1:-}" = "--strict" ] && strict=true

# corpus label | env var | needs-firmware note
corpora=(
  "Tom Harte 6502|EMU198X_6502_TOM_HARTE_DIR|"
  "Tom Harte Z80|EMU198X_Z80_TOM_HARTE_DIR|"
  "Tom Harte 68000|EMU198X_68000_TOM_HARTE_ROOT|"
  "SM83 (Tennant)|EMU198X_SM83_TENNANT_DIR|"
  "Klaus Dormann 6502|EMU198X_6502_DORMANN_DIR|"
  "FUSE Z80|EMU198X_FUSE_Z80_TESTS_DIR|"
  "Wolfgang Lorenz 6502|EMU198X_6502_LORENZ_DIR|also needs EMU198X_C64_KERNAL_ROM"
)

printf '%-22s %-32s %-8s %s\n' "CORPUS" "ENV VAR" "STATUS" "NOTE"
printf '%-22s %-32s %-8s %s\n' "------" "-------" "------" "----"

missing=0
for entry in "${corpora[@]}"; do
  IFS='|' read -r label var note <<< "${entry}"
  path="${!var:-}"
  if [ -z "${path}" ]; then
    status="unset"
    missing=$((missing + 1))
  elif [ -e "${path}" ]; then
    status="present"
  else
    status="MISSING"
    note="path does not exist: ${path}"
    missing=$((missing + 1))
  fi
  printf '%-22s %-32s %-8s %s\n' "${label}" "${var}" "${status}" "${note}"
done

echo
if [ "${missing}" -eq 0 ]; then
  echo "All ${#corpora[@]} corpora reachable."
  exit 0
fi

echo "${missing}/${#corpora[@]} corpora not reachable — the matching ignored tests will not run."
echo "See test-data/accuracy-corpora.md for sources and the mirror store."
if [ "${strict}" = true ]; then
  exit 1
fi
exit 0

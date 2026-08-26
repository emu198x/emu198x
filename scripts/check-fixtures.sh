#!/usr/bin/env bash

# Reports which external accuracy corpora are reachable, by checking each
# corpus's env var and target path. Used two ways:
#   - locally, to see what the ignored CPU-conformance tests can run against;
#   - in CI, as a preflight before the nightly-accuracy workflow's test steps.
#
# The corpus contract (env var per corpus, sources, the mirror store) is
# documented in test-data/accuracy-corpora.md — keep this list in sync with it.
#
# # Local defaults
#
# Most of these corpora are already checked out in the 198x umbrella — Tom
# Harte's under `assets/test-suites/processor-tests/`, FUSE's inside the
# vendored `emulators/zx-spectrum/fuse-emulator-fuse/` tree. Nothing pointed
# the env vars at them, so `cargo test` skipped the suites and this script
# reported them unset, on machines that had the data all along. That is the
# same class of gap as a corpus nobody runs: the tests are green because they
# did not execute.
#
# So each corpus below carries a default path *and a sentinel file* that must
# exist inside it. The sentinel is what makes the default trustworthy — a
# directory existing proves nothing about what is in it.
#
# Run with `--export` to print the `export` lines for everything resolved by
# default, which is the quickest way to make a local run match CI:
#
#     eval "$(scripts/check-fixtures.sh --export)"
#
# Exit codes: 0 if every corpus is present (or in report mode); 1 in --strict
# mode when any corpus is missing.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

# The org container's parent — `198x/`, holding `assets/` and `emulators/`.
umbrella="$(cd "${repo_root}/../.." 2>/dev/null && pwd || echo "")"
store="${HOME}/.emu198x/test-data"

mode="report"
case "${1:-}" in
  --strict) mode="strict" ;;
  --export) mode="export" ;;
esac

# corpus label | env var | default path | sentinel file inside it | note
corpora=(
  "Tom Harte 6502|EMU198X_6502_TOM_HARTE_DIR|${umbrella}/assets/test-suites/processor-tests/65x02/6502/v1|00.json|"
  "Tom Harte Z80|EMU198X_Z80_TOM_HARTE_DIR|${umbrella}/assets/test-suites/processor-tests/z80/v1|00.json|"
  "Tom Harte 68000|EMU198X_68000_TOM_HARTE_ROOT|${umbrella}/assets/test-suites/processor-tests/680x0/68000/v1|ADD.b.json.gz|"
  "SM83 (Tennant)|EMU198X_SM83_TENNANT_DIR|${umbrella}/assets/test-suites/processor-tests/sm83/v1|cb.json|sentinel is cb.json, not 00.json: Harte's SM83 tree has 00.json too, and the two disagree on where initial.pc points (#1230)"
  "Klaus Dormann 6502|EMU198X_6502_DORMANN_DIR|${umbrella}/assets/test-suites/6502|6502_functional_test.bin|"
  "FUSE Z80|EMU198X_FUSE_Z80_TESTS_DIR|${umbrella}/emulators/zx-spectrum/fuse-emulator-fuse/z80/tests|tests.in|"
  "Wolfgang Lorenz 6502|EMU198X_6502_LORENZ_DIR|${umbrella}/assets/test-suites/vice/bin|adca|also needs a KERNAL; the store's tarball carries a synthetic one"
  "ZEXDOC + ZEXALL|EMU198X_ZEX_DIR|${umbrella}/assets/test-suites/zex|zexdoc.com|"
  "z80test (Rak)|EMU198X_Z80TEST_DIR|${store}/z80test|z80doc.tap|48K ROM comes from the tarball in CI; see below"
  "Spectrum system tests|EMU198X_SPECTRUM_SYSTEM_TESTS_DIR|${store}/spectrum-system-tests|floatspy.tap|"
)

exports=()
missing=0
rows=()

for entry in "${corpora[@]}"; do
  IFS='|' read -r label var default sentinel note <<< "${entry}"
  path="${!var:-}"
  if [ -n "${path}" ]; then
    if [ -e "${path}" ]; then
      status="present"
    else
      status="MISSING"
      note="path does not exist: ${path}"
      missing=$((missing + 1))
    fi
  elif [ -n "${default}" ] && [ -e "${default}/${sentinel}" ]; then
    status="default"
    exports+=("export ${var}='${default}'")
    [ -z "${note}" ] && note="using ${default#"${umbrella}/"}"
  else
    status="unset"
    [ -z "${note}" ] && note="no default found"
    missing=$((missing + 1))
  fi
  rows+=("$(printf '%-22s %-38s %-8s %s' "${label}" "${var}" "${status}" "${note}")")
done

if [ "${mode}" = "export" ]; then
  printf '%s\n' "${exports[@]:-}"
  exit 0
fi

printf '%-22s %-38s %-8s %s\n' "CORPUS" "ENV VAR" "STATUS" "NOTE"
printf '%-22s %-38s %-8s %s\n' "------" "-------" "------" "----"
printf '%s\n' "${rows[@]}"

# The z80test corpus is the one place where "present" is not the whole story:
# CI points EMU198X_SPECTRUM_48K_ROM at a 48.rom inside the tarball, and the
# local staging has only the tapes. Same suite, different ROM, nothing says so.
if [ -n "${EMU198X_Z80TEST_DIR:-${store}/z80test}" ] &&
   [ ! -e "${EMU198X_Z80TEST_DIR:-${store}/z80test}/48.rom" ]; then
  echo
  echo "note: z80test has no 48.rom beside its tapes, so a local run uses"
  echo "      whatever EMU198X_SPECTRUM_48K_ROM points at rather than the ROM"
  echo "      CI bundles. Results are comparable only if those match."
fi

echo
if [ "${#exports[@]}" -gt 0 ]; then
  echo "${#exports[@]} corpora resolved from local checkouts. To use them:"
  echo "    eval \"\$(scripts/check-fixtures.sh --export)\""
  echo
fi

if [ "${missing}" -eq 0 ]; then
  echo "All ${#corpora[@]} corpora reachable."
  exit 0
fi

echo "${missing}/${#corpora[@]} corpora not reachable — the matching ignored tests will not run."
echo "See test-data/accuracy-corpora.md for sources and the mirror store."
if [ "${mode}" = "strict" ]; then
  exit 1
fi
exit 0

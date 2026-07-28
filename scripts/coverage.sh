#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mkdir -p target/llvm-cov

# Branch coverage was dropped 2026-04-30: it requires `+nightly`, and
# the nightly-instrumented build was producing roughly 2x the
# "functions have mismatched data" warnings of a stable run. Region,
# function, and line coverage are sufficient signal for the
# directed-test passes we run, and stable instrumentation is more
# coherent across parallel agent runs. If branch coverage matters
# again later, restore `coverage_toolchain="+nightly"` and
# `coverage_flags=(--branch)`.
#
# The motorola-68000 family was split into per-variant crates in April
# 2026: motorola-68k-common, motorola-68000, motorola-68010,
# motorola-68020, motorola-68030, and motorola-68040. The previous
# Cov-3 carve-out for `motorola-68000/src/(fpu|mmu|disasm)\.rs` no
# longer applies — disasm.rs was deleted, fpu.rs moved to
# motorola-68040, and mmu.rs moved to motorola-68k-common (and is
# re-exported from motorola-68030). The dormant variant crates carry
# their own coverage numbers and don't artificially depress the
# motorola-68000 figure. Tom Harte continues to validate the live
# 68000 opcode surface at 1,000,058/1,000,058.

# Run instrumented tests, then generate reports separately. Three
# things this fixes against the previous script shape:
#   1. `--lib --tests` ensures integration tests under `tests/` run
#      too. Without this, `cargo llvm-cov` defaults to the lib-only
#      target set and reports any crate whose coverage comes from
#      integration tests (i.e. most runtimes after the Cov-5b track)
#      as effectively 0%.
#   2. `--no-report` here pairs with the explicit
#      `cargo llvm-cov report` invocations below, whose stdout is the
#      formatted per-file + TOTAL summary. The previous shape piped
#      the test-running invocation through `tee`, which writes its
#      progress messages to stderr, so the summary file came out
#      empty.
#   3. `--no-fail-fast` + deferred failure gating: a single failing
#      test in any crate (often Codex iterating on Dragon code) used
#      to kill the entire run before any reports were generated. Now
#      the run completes (so the reports are always produced), the
#      failure is recorded, and the script exits non-zero at the very
#      end — so failing tests still gate CI without costing the report.
#
# `--include-ignored` opts in to the CPU-corpus regressions
# (Tom Harte 6502/Z80/68000, Wolfgang Lorenz, Dormann, ZEX, Adam
# Tennant SM83, mooneye Game Boy diagnostics, Amiga Kickstart
# fixtures, …). They're `#[ignore]`'d in normal runs because they
# require external fixtures (1 GiB+ corpora outside the repo) and
# run for minutes. With the flag, llvm-cov instruments them and the
# resulting coverage figures reflect the broader instruction-set
# surface — at the cost of a substantially longer wall-clock run
# and disk-space pressure. Many optional corpus tests self-skip when
# their fixtures are absent. Explicit accuracy gates, including both
# Amiga Test Kit lanes, fail when requested without their registered
# inputs; provision those fixtures or invoke their dedicated wrappers
# instead of assuming a global ignored-test run will skip them.
extra_cargo_args=()
libtest_args=()
for arg in "$@"; do
    case "$arg" in
        --include-ignored)
            libtest_args+=(--include-ignored)
            ;;
        *)
            extra_cargo_args+=("$arg")
            ;;
    esac
done

cargo_cmd=(
    cargo llvm-cov --workspace --lib --tests --no-fail-fast --no-report
)
if [ ${#extra_cargo_args[@]} -gt 0 ]; then
    cargo_cmd+=("${extra_cargo_args[@]}")
fi
if [ ${#libtest_args[@]} -gt 0 ]; then
    cargo_cmd+=(-- "${libtest_args[@]}")
fi

test_status=0
"${cargo_cmd[@]}" || test_status=$?

if [ "${test_status}" -ne 0 ]; then
    echo
    echo "WARNING: cargo llvm-cov exited with status ${test_status} —" \
         "one or more tests failed. Coverage data is still complete" \
         "(tests run to completion under --no-fail-fast); the reports" \
         "below are generated regardless, then the script exits" \
         "non-zero at the end so the failure gates CI."
    echo
fi

cargo llvm-cov report | tee target/llvm-cov/coverage-summary.txt
cargo llvm-cov report --json --summary-only \
    --output-path target/llvm-cov/coverage-summary.json
cargo llvm-cov report --lcov \
    --output-path target/llvm-cov/lcov.info
cargo llvm-cov report --html --output-dir target/llvm-cov

echo
echo "Coverage total:"
grep '^TOTAL' target/llvm-cov/coverage-summary.txt | tail -n 1
echo
echo "Reports:"
echo "  target/llvm-cov/coverage-summary.txt"
echo "  target/llvm-cov/coverage-summary.json"
echo "  target/llvm-cov/lcov.info"
echo "  target/llvm-cov/html/index.html"

# Gate the build on test results. The reports above are always produced
# first — a failing test never costs us the coverage data — but a non-zero
# test status now fails the script, so failing tests gate CI as intended.
if [ "${test_status}" -ne 0 ]; then
    echo
    echo "FAILED: test run exited with status ${test_status} —" \
         "one or more tests failed (see the output above)." >&2
    exit "${test_status}"
fi

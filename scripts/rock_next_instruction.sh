#!/usr/bin/env bash
#
# Runs the cpu-m68k-rock single-step tests and recommends which instruction
# to work on next. Prioritises finishing individual instructions (those with
# the highest pass rate) over broad coverage.
#
# Usage:
#   ./scripts/rock_next_instruction.sh             # run suite + report
#   ./scripts/rock_next_instruction.sh --analyze    # re-analyze last run (no cargo test)
#   ./scripts/rock_next_instruction.sh --errors     # include sample errors in report
#   ./scripts/rock_next_instruction.sh TST.b        # run just one test file (fast)
#   ./scripts/rock_next_instruction.sh TST.b --errors  # one file + all errors

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CACHE_FILE="${SCRIPT_DIR}/../test_output.log"

show_errors=false
analyze_only=false
single_file=""

for arg in "$@"; do
    case "$arg" in
        --errors) show_errors=true ;;
        --analyze) analyze_only=true ;;
        *) single_file="$arg" ;;
    esac
done

# --- Single file mode: fast, targeted ---
if [[ -n "$single_file" ]]; then
    echo "Running ${single_file}..."
    ROCK_TEST_FILE="$single_file" cargo test -p cpu-m68k-rock \
        --test single_step_tests run_single_file \
        -- --ignored --nocapture 2>&1
    exit $?
fi

# --- Full suite ---
if [[ "$analyze_only" == true ]]; then
    if [[ ! -f "$CACHE_FILE" ]]; then
        echo "No cached results at $CACHE_FILE — run without --analyze first."
        exit 1
    fi
    echo "(Using cached results from $CACHE_FILE)"
    output=$(cat "$CACHE_FILE")
else
    echo "Running full test suite..."
    output=$(cargo test -p cpu-m68k-rock --test single_step_tests run_all_single_step_tests \
        -- --ignored --nocapture 2>&1)
    echo "$output" > "$CACHE_FILE"
    echo "(Results cached to test_output.log)"
fi

# Extract total
total_line=$(echo "$output" | grep '^=== Total:' || echo "(no total found)")
echo ""
echo "$total_line"
echo ""

# Parse per-file results into a temp file
tmpfile=$(mktemp)
echo "$output" | grep -E '^\w.*\.json:' | while IFS= read -r line; do
    # Format: "NAME.json: N passed, M failed" or "NAME.json: N passed"
    name=$(echo "$line" | sed 's/\.json:.*//')
    passed=$(echo "$line" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+')
    failed=$(echo "$line" | grep -oE '[0-9]+ failed' | grep -oE '[0-9]+' || echo "0")
    total=$((passed + failed))
    if [[ $total -gt 0 ]]; then
        pct=$((passed * 100 / total))
    else
        pct=0
    fi
    echo "$pct $passed $failed $total $name" >> "$tmpfile"
done

# Sort: 100% first, then by pass rate descending
sort -t' ' -k1,1rn -k2,2rn "$tmpfile" > "${tmpfile}.sorted"

# --- Report ---
echo "=== COMPLETED (100%) ==="
awk '$1 == 100 { printf "  %-20s %d/%d\n", $5, $2, $4 }' "${tmpfile}.sorted"
completed=$(awk '$1 == 100' "${tmpfile}.sorted" | wc -l | tr -d ' ')
echo "  ($completed instructions at 100%)"
echo ""

echo "=== ALMOST DONE (>50%) — fix these first ==="
awk '$1 > 50 && $1 < 100 { printf "  %-20s %d/%d (%d%%) — %d to fix\n", $5, $2, $4, $1, $3 }' "${tmpfile}.sorted"
echo ""

echo "=== PARTIAL (1-50%) ==="
awk '$1 >= 1 && $1 <= 50 { printf "  %-20s %d/%d (%d%%)\n", $5, $2, $4, $1 }' "${tmpfile}.sorted"
echo ""

echo "=== NOT STARTED (0%) ==="
awk '$1 == 0 { printf "  %s", $5 } END { printf "\n" }' "${tmpfile}.sorted" | fold -s -w 72
not_started=$(awk '$1 == 0' "${tmpfile}.sorted" | wc -l | tr -d ' ')
echo "  ($not_started instructions at 0%)"
echo ""

# --- Recommendation ---
echo "=== RECOMMENDATION ==="
best=$(awk '$1 > 0 && $1 < 100 { print; exit }' "${tmpfile}.sorted")
if [[ -n "$best" ]]; then
    best_name=$(echo "$best" | awk '{print $5}')
    best_pct=$(echo "$best" | awk '{print $1}')
    best_pass=$(echo "$best" | awk '{print $2}')
    best_fail=$(echo "$best" | awk '{print $3}')
    echo "  Focus on: $best_name ($best_pass passed, $best_fail to fix, $best_pct%)"
    echo ""

    if [[ "$show_errors" == true ]]; then
        echo "  First errors for $best_name:"
        echo "$output" | grep -A20 "^${best_name}.json:" | head -10
    fi

    echo "  Quick test: ./scripts/rock_next_instruction.sh $best_name"
else
    echo "  All implemented instructions pass! Time to add new ones."
fi

# Cleanup
rm -f "$tmpfile" "${tmpfile}.sorted"

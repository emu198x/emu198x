#!/usr/bin/env bash
#
# Spectrum-side coverage gate.
#
# Reads target/llvm-cov/coverage-summary.json (produced by
# scripts/coverage.sh) and fails the run if any Spectrum-specific
# crate is below the line-coverage threshold.
#
# Threshold defaults to 75 (the current low-water mark for in-scope
# crates as of 2026-05-07). The SOLID criterion 11 target is 90; we
# ratchet up as new tests land. Overridden via the
# COVERAGE_GATE_THRESHOLD env var.
#
# The "Spectrum-specific" scope is the set of crates that exist
# primarily for the Spectrum family. Some chip crates are shared with
# future systems (emu198x-zilog-z80 also covers MSX / CPC / Master System;
# gi-ay-3-8912 also covers MSX / CPC / Mockingboard), but they ship
# today as Spectrum-driving infrastructure and are part of the
# October-launch quality bar.
#
# The gate splits the scope in two:
#   - GATED:    the 8 in-scope October-public variants and their
#               shared infrastructure. Must meet the threshold.
#   - REPORTED: family-bar variants (Pentagon, Scorpion, Timex, Beta)
#               with no October deadline. Coverage is shown for
#               visibility but not enforced. They graduate to GATED
#               when the engineering bar work for those variants
#               lands.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

threshold="${COVERAGE_GATE_THRESHOLD:-75}"
summary="target/llvm-cov/coverage-summary.json"

if [ ! -f "${summary}" ]; then
    echo "ERROR: ${summary} not found. Run scripts/coverage.sh first." >&2
    exit 2
fi

python3 - "${summary}" "${threshold}" <<'PY'
import json
import re
import sys
from pathlib import Path

summary_path = Path(sys.argv[1])
threshold = float(sys.argv[2])

# Spectrum-specific crate paths. Anchored on `crates/<name>` in the
# filename so a future move under `crates/foo/spectrum/` doesn't
# accidentally widen the scope.
#
# Matched against the filename (which is an absolute path emitted by
# llvm-cov). The regexes are deliberately verbose — easy to scan and
# easy to edit when a new variant lands.
# Crates the gate enforces — October-public scope (8 in-scope
# variants and their shared infrastructure).
GATED_CRATES = [
    # Family commons + class layers
    "common-sinclair-zx-spectrum",
    "common-sinclair-zx-spectrum-48k-class",
    "common-sinclair-zx-spectrum-128k-class",
    "common-sinclair-zx-spectrum-amstrad-class",
    # Chip crates the Spectrum drives today
    "amstrad-ula-40077",
    "ferranti-ula-6c001e",
    "gi-ay-3-8912",
    "nec-upd765a",
    "sinclair-ula-7k010e",
    "emu198x-zilog-z80",
    # Format crates
    "format-amstrad-dsk",
    "format-sinclair-zx-spectrum-sna",
    "format-sinclair-zx-spectrum-snapshot",
    "format-sinclair-zx-spectrum-tap",
    "format-sinclair-zx-spectrum-tzx",
    "format-sinclair-zx-spectrum-z80",
    # Machine crates — the 8 in-scope October-public variants
    "machine-sinclair-zx-spectrum-16k",
    "machine-sinclair-zx-spectrum-48k",
    "machine-sinclair-zx-spectrum-128k",
    "machine-sinclair-zx-spectrum-plus",
    "machine-sinclair-zx-spectrum-plus2",
    "machine-sinclair-zx-spectrum-plus2a",
    "machine-sinclair-zx-spectrum-plus2b",
    "machine-sinclair-zx-spectrum-plus3",
    # Peripheral crates
    "peripheral-kempston-joystick",
    # Runtime
    "runtime-sinclair-zx-spectrum",
]

# Crates the gate reports but does not enforce — family-bar variants
# without an October deadline. Promoted into GATED_CRATES when their
# engineering bar work lands.
REPORTED_CRATES = [
    "beta-disk-interface",
    "machine-pentagon-128",
    "machine-scorpion-zs256",
    "machine-timex-tc2048",
    "machine-timex-ts2068",
    "pentagon-ula",
    "scorpion-ula",
    "timex-scld",
]

# Data-only crates exempt from the line-coverage gate per
# knowledge/systems/spectrum/solid-status.md (criterion 11). cargo-llvm-cov
# reports 0 / 0 lines for them, which would be undefined under the
# gate's coverage formula anyway.
EXEMPT_CRATES = {
    "format-sinclair-zx-spectrum-snapshot",
}

# Group files by crate. Match `crates/<name>/` in the path so the
# longest-name match wins (e.g. `common-sinclair-zx-spectrum-48k-class`
# beats `common-sinclair-zx-spectrum`).
crate_re = re.compile(r"/crates/([^/]+)/")
gated_set = set(GATED_CRATES)
reported_set = set(REPORTED_CRATES)
all_set = gated_set | reported_set

per_crate: dict[str, dict[str, int]] = {}

with summary_path.open() as fh:
    data = json.load(fh)

for entry in data["data"][0]["files"]:
    filename = entry["filename"]
    match = crate_re.search(filename)
    if not match:
        continue
    crate = match.group(1)
    if crate not in all_set:
        continue
    if crate in EXEMPT_CRATES:
        continue
    line = entry["summary"]["lines"]
    bucket = per_crate.setdefault(
        crate, {"count": 0, "covered": 0, "files": 0}
    )
    bucket["count"] += line["count"]
    bucket["covered"] += line["covered"]
    bucket["files"] += 1


def render(title: str, crates: list[str], enforce: bool) -> list[tuple[str, float]]:
    print(title)
    print()
    print(f"{'crate':<48} {'lines':>10} {'percent':>10}")
    print("-" * 70)
    failing: list[tuple[str, float]] = []
    for crate in sorted(crates):
        bucket = per_crate.get(crate)
        if bucket is None or bucket["count"] == 0:
            print(f"{crate:<48} {'-':>10}   no data")
            continue
        pct = 100.0 * bucket["covered"] / bucket["count"]
        line_str = f"{bucket['covered']}/{bucket['count']}"
        pct_str = f"{pct:.2f}%"
        suffix = ""
        if pct < threshold:
            if enforce:
                suffix = "  BELOW THRESHOLD"
                failing.append((crate, pct))
            else:
                suffix = "  (informational, not gated)"
        print(f"{crate:<48} {line_str:>10} {pct_str:>10}{suffix}")
    print()
    return failing


print(f"Spectrum-side coverage gate (threshold: {threshold:.1f}%)")
print()

below = render(
    "Gated — October-public scope:",
    GATED_CRATES,
    enforce=True,
)

render(
    "Reported only — family-bar variants without October deadline:",
    REPORTED_CRATES,
    enforce=False,
)

# Gated crates absent from the coverage report — usually means tests
# are not running or the crate is misconfigured.
seen = set(per_crate)
missing = sorted(gated_set - seen - EXEMPT_CRATES)

if missing:
    print("Gated crates with no coverage data:")
    for crate in missing:
        print(f"  - {crate}")
    print()
    print(
        f"FAIL: {len(missing)} gated crate(s) have no coverage data — "
        f"either tests are not running or the crate is misconfigured."
    )
    sys.exit(1)

if below:
    print(
        f"FAIL: {len(below)} gated crate(s) below {threshold:.1f}% line coverage:"
    )
    for crate, pct in below:
        print(f"  - {crate} at {pct:.2f}%")
    sys.exit(1)

print(f"OK: all {len(GATED_CRATES) - len(EXEMPT_CRATES & gated_set)} gated crates meet the {threshold:.1f}% threshold.")
PY

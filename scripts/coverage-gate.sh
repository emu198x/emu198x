#!/usr/bin/env bash
#
# Spectrum-side coverage gate.
#
# Reads target/llvm-cov/coverage-summary.json (produced by
# scripts/coverage.sh) and fails the run if any Spectrum-specific
# crate is below the line-coverage threshold.
#
# Threshold defaults to 90 (per SOLID criterion 11) and can be
# overridden via the COVERAGE_GATE_THRESHOLD env var.
#
# The "Spectrum-specific" scope is the set of crates that exist
# primarily for the Spectrum family. Some chip crates are shared with
# future systems (zilog-z80 also covers MSX / CPC / Master System;
# gi-ay-3-8912 also covers MSX / CPC / Mockingboard), but they ship
# today as Spectrum-driving infrastructure and are part of the
# October-launch quality bar. The list is the authoritative scope of
# the SOLID coverage criterion until the catalogue widens to other
# systems.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

threshold="${COVERAGE_GATE_THRESHOLD:-90}"
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
SPECTRUM_CRATES = [
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
    "pentagon-ula",
    "scorpion-ula",
    "sinclair-ula-7k010e",
    "timex-scld",
    "zilog-z80",
    # Format crates
    "format-amstrad-dsk",
    "format-sinclair-zx-spectrum-sna",
    "format-sinclair-zx-spectrum-snapshot",
    "format-sinclair-zx-spectrum-tap",
    "format-sinclair-zx-spectrum-tzx",
    "format-sinclair-zx-spectrum-z80",
    # Machine crates
    "machine-pentagon-128",
    "machine-scorpion-zs256",
    "machine-sinclair-zx-spectrum-16k",
    "machine-sinclair-zx-spectrum-48k",
    "machine-sinclair-zx-spectrum-128k",
    "machine-sinclair-zx-spectrum-plus",
    "machine-sinclair-zx-spectrum-plus2",
    "machine-sinclair-zx-spectrum-plus2a",
    "machine-sinclair-zx-spectrum-plus2b",
    "machine-sinclair-zx-spectrum-plus3",
    "machine-timex-tc2048",
    "machine-timex-ts2068",
    # Peripheral crates
    "beta-disk-interface",
    "peripheral-kempston-joystick",
    # Runtime
    "runtime-sinclair-zx-spectrum",
]

# Data-only crates exempt from the line-coverage gate per
# wiki/systems/spectrum/solid-status.md (criterion 11). cargo-llvm-cov
# reports 0 / 0 lines for them, which would be undefined under the
# gate's coverage formula anyway.
EXEMPT_CRATES = {
    "format-sinclair-zx-spectrum-snapshot",
}

# Group files by crate. Match `crates/<name>/` in the path so the
# longest-name match wins (e.g. `common-sinclair-zx-spectrum-48k-class`
# beats `common-sinclair-zx-spectrum`).
crate_re = re.compile(r"/crates/([^/]+)/")
crate_set = set(SPECTRUM_CRATES)

per_crate: dict[str, dict[str, int]] = {}

with summary_path.open() as fh:
    data = json.load(fh)

for entry in data["data"][0]["files"]:
    filename = entry["filename"]
    match = crate_re.search(filename)
    if not match:
        continue
    crate = match.group(1)
    if crate not in crate_set:
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

# Compute per-crate percentages, sort by ascending coverage so the
# worst offenders surface first.
results = []
for crate in sorted(per_crate):
    bucket = per_crate[crate]
    if bucket["count"] == 0:
        # No executable lines — should be in EXEMPT_CRATES if expected.
        # Surface as a warning so scope drift is visible.
        results.append((crate, None, bucket["files"]))
        continue
    pct = 100.0 * bucket["covered"] / bucket["count"]
    results.append((crate, pct, bucket["files"]))

# Crates listed in scope but absent from the coverage report — usually
# means they have no tests at all, or weren't exercised by the run.
seen = set(per_crate)
missing = sorted(crate_set - seen - EXEMPT_CRATES)

print(f"Spectrum-side coverage gate (threshold: {threshold:.1f}%)")
print()
print(f"{'crate':<48} {'lines':>10} {'percent':>10}")
print("-" * 70)

below = []
for crate, pct, _ in results:
    if pct is None:
        marker = "  no executable lines"
        print(f"{crate:<48} {'-':>10} {marker}")
        continue
    bucket = per_crate[crate]
    line_str = f"{bucket['covered']}/{bucket['count']}"
    pct_str = f"{pct:.2f}%"
    suffix = ""
    if pct < threshold:
        suffix = "  BELOW THRESHOLD"
        below.append((crate, pct))
    print(f"{crate:<48} {line_str:>10} {pct_str:>10}{suffix}")

if missing:
    print()
    print("Spectrum-side crates with no coverage data:")
    for crate in missing:
        print(f"  - {crate}")

print()

if missing:
    print(
        f"FAIL: {len(missing)} Spectrum-side crate(s) have no coverage data — "
        f"either tests are not running or the crate is misconfigured."
    )
    sys.exit(1)

if below:
    print(
        f"FAIL: {len(below)} crate(s) below {threshold:.1f}% line coverage:"
    )
    for crate, pct in below:
        print(f"  - {crate} at {pct:.2f}%")
    sys.exit(1)

print(f"OK: all {len(results)} Spectrum-side crates meet the {threshold:.1f}% threshold.")
PY

"""Compile registered DDF functions for the connected boundary regressions."""

import argparse
import contextlib
import csv
import hashlib
import importlib.util
import io
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def build(source: Path, output: Path) -> None:
    spec = importlib.util.spec_from_file_location(
        "register_probe", ROOT.parent / "ddf-register-writes/build-reference.py"
    )
    if spec is None or spec.loader is None:
        raise SystemExit("cannot load registered DDF extractor")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # This verifies the pinned source hash, unchanged fragments and do_cck order.
    with contextlib.redirect_stdout(io.StringIO()):
        module.build(source, output / "registers", False)
    prefix = (
        (output / "registers/reference-writes.cpp").read_text().split("int main(")[0]
    )
    cpp = output / "reference.cpp"
    cpp.write_text(prefix + (ROOT / "probe.cpp.in").read_text())
    binary = output / "reference"
    subprocess.run(
        ["clang++", "-std=c++17", "-O2", str(cpp), "-o", str(binary)], check=True
    )
    normal = subprocess.run([str(binary)], capture_output=True, text=True, check=True)
    negative = subprocess.run(
        [str(binary), "reverse"], capture_output=True, text=True, check=True
    )

    def grouped(data: str) -> dict[int, list[tuple[int, int, int, int]]]:
        groups: dict[int, list[tuple[int, int, int, int]]] = {i: [] for i in range(7)}
        for row in csv.DictReader(io.StringIO(data)):
            groups[int(row["case"])].append(
                tuple(int(row[key]) for key in ("line", "h", "plane", "mod"))
            )
        if any(not rows for rows in groups.values()):
            raise SystemExit("missing boundary request coverage")
        return groups

    expected, broken = grouped(normal.stdout), grouped(negative.stdout)
    differing = [case for case in expected if expected[case] != broken[case]]
    if len(differing) != 7:
        raise SystemExit(
            "reversed request/comparator control must fail all seven cases"
        )

    def first_line(
        rows: list[tuple[int, int, int, int]],
    ) -> list[tuple[int, int, int, int]]:
        return [row for row in rows if row[0] == 0]

    if first_line(expected[0]) != first_line(expected[1]):
        raise SystemExit("hard-stop rewrite revoked the terminal fetch")
    early = [row for row in expected[2] if row[0] == 2]
    if early[:4] != [(2, 18, 3, 0), (2, 19, 1, 0), (2, 20, 2, 0), (2, 21, 0, 0)]:
        raise SystemExit("idle-line early-start schedule differs")
    if [len(first_line(expected[i])) for i in range(7)] != [
        25,
        25,
        189,
        200,
        201,
        168,
        169,
    ]:
        raise SystemExit("boundary request counts differ")
    (output / "registered-events.csv").write_text(normal.stdout)
    report = {
        "source_sha256": module.SOURCE_SHA256,
        "probe_sha256": hashlib.sha256(
            (ROOT / "probe.cpp.in").read_bytes()
        ).hexdigest(),
        "events_sha256": hashlib.sha256(normal.stdout.encode()).hexdigest(),
        "cases": 7,
        "requests": sum(map(len, expected.values())),
        "reversed_order_mismatching_cases": differing,
        "scope": "Unchanged reference handlers and sequencer; external signals; no RGA/memory or silicon claim",
    }
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    build(args.source, args.output)

"""Reproduce the early AGA wide-fetch schedule using registered source functions."""

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
    with contextlib.redirect_stdout(io.StringIO()):
        module.build(source, output / "registers", False)
    prefix = (
        (output / "registers/reference-writes.cpp").read_text().split("int main(")[0]
    )
    cpp = output / "wide-boundary.cpp"
    cpp.write_text(prefix + (ROOT / "wide-boundary.cpp.in").read_text())
    binary = output / "wide-boundary"
    subprocess.run(
        ["clang++", "-std=c++17", "-O2", str(cpp), "-o", str(binary)], check=True
    )
    normal = subprocess.run([str(binary)], capture_output=True, text=True, check=True)
    negative = subprocess.run(
        [str(binary), "reverse"], capture_output=True, text=True, check=True
    )

    def events(data: str) -> list[tuple[int, int, int]]:
        return [
            (int(row["h"]), int(row["plane"]), int(row["mod"]))
            for row in csv.DictReader(io.StringIO(data))
        ]

    expected = [(20 + i, plane, 0) for i, plane in enumerate([7, 3, 5, 1, 6, 2, 4, 0])]
    if events(normal.stdout) != expected:
        raise SystemExit("registered early-wide request schedule differs")
    if events(negative.stdout) == expected:
        raise SystemExit("reversed-order control did not detect a scheduling error")
    (output / "registered-events.csv").write_text(normal.stdout)
    report = {
        "source_sha256": module.SOURCE_SHA256,
        "probe_sha256": hashlib.sha256(
            (ROOT / "wide-boundary.cpp.in").read_bytes()
        ).hexdigest(),
        "events_sha256": hashlib.sha256(normal.stdout.encode()).hexdigest(),
        "requests": len(expected),
        "reversed_order_differs": True,
        "scope": "AGA FMODE=1 lores eight-plane HARDDIS request schedule; no RGA service, raster or silicon claim",
    }
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    build(args.source, args.output)

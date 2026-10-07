"""Measure DDF writes using unchanged registered handlers and sequencer code."""

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
SOURCE_SHA256 = "75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build(source_path: Path, output: Path, immediate: bool) -> None:
    original = source_path.read_bytes()
    if digest(original) != SOURCE_SHA256:
        raise SystemExit("registered custom.cpp source hash differs")
    source = original.decode()
    spec = importlib.util.spec_from_file_location(
        "sequencer_probe", ROOT.parent / "display-dma-sequencer/build-reference.py"
    )
    if spec is None or spec.loader is None:
        raise SystemExit("cannot load existing sequencer probe")
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    output.mkdir(parents=True, exist_ok=True)
    with contextlib.redirect_stdout(io.StringIO()):
        base.build(source_path, output / "sequencer", False)
    fragments = {
        "pipeline_type": base.extract(
            source, "struct pipeline_reg\n", "static uae_u32 displayresetcnt;"
        ),
        "pipeline": base.extract(
            source,
            "static void empty_pipeline(void)",
            "static void pipelined_custom_write(",
        ),
        "writes": base.extract(
            source, "static void DDFSTRT(uae_u16 v)", "static void FMODE("
        ),
    }
    cck = base.extract(
        source, "static void do_cck(bool docycles)", "// horizontal sync callback"
    )
    positions = [
        cck.index(part)
        for part in (
            "handle_rga_out();",
            "generate_dma_requests();",
            "decide_bpl(agnus_hpos);",
            "empty_pipeline();",
            "inc_cck();",
        )
    ]
    if positions != sorted(positions):
        raise SystemExit("registered write/comparator/pipeline ordering differs")
    prefix = (
        (output / "sequencer/reference-sequencer.cpp").read_text().split("int main(")[0]
    )
    declaration = "int bprun,bprun_cycle,ddf_stopping,ddf_enable_on,ddfstrt,ddfstop;"
    if prefix.count(declaration) != 1:
        raise SystemExit("sequencer probe declarations changed")
    prefix = prefix.replace(
        declaration,
        "int bprun,bprun_cycle,ddf_stopping,ddf_enable_on;\n"
        "using uae_u16=uint16_t;\nuae_u16 ddfstrt,ddfstop,ddfstrt_saved,ddfstop_saved,ddf_mask;",
    )
    cpp = output / "reference-writes.cpp"
    cpp.write_text(
        prefix
        + fragments["pipeline_type"]
        + "\nstatic pipeline_reg preg;\n"
        + fragments["pipeline"]
        + fragments["writes"]
        + (ROOT / "probe.cpp.in").read_text()
    )
    binary = output / "reference-writes"
    subprocess.run(
        ["clang++", "-std=c++17", "-O2", str(cpp), "-o", str(binary)], check=True
    )
    expected = subprocess.run([str(binary)], capture_output=True, text=True, check=True)
    broken = subprocess.run(
        [str(binary), "immediate"], capture_output=True, text=True, check=True
    )
    header = "case,chip,line_ccks,phase,reg,scenario,write_h,value,start,stop\n"
    cases = list(csv.DictReader(io.StringIO(header + expected.stderr)))
    events = list(csv.DictReader(io.StringIO(expected.stdout)))
    if len(cases) != 1128 or len({row["case"] for row in cases}) != 1128 or not events:
        raise SystemExit("missing case or request coverage")
    if expected.stderr != broken.stderr:
        raise SystemExit("negative control changed case definitions")

    def grouped(data: str) -> dict[str, list[dict[str, str]]]:
        result = {row["case"]: [] for row in cases}
        for row in csv.DictReader(io.StringIO(data)):
            result[row["case"]].append(row)
        return result

    actual_groups, broken_groups = grouped(expected.stdout), grouped(broken.stdout)
    differing = [
        row["case"]
        for row in cases
        if actual_groups[row["case"]] != broken_groups[row["case"]]
    ]
    controls = [row["case"] for row in cases if row["scenario"] == "6"]
    if not differing or any(case in differing for case in controls):
        raise SystemExit("immediate-write negative control failed to discriminate")
    connected_cpp = output / "connected.cpp"
    connected_cpp.write_text(
        cpp.read_text().split("int main(")[0] + (ROOT / "connected.cpp.in").read_text()
    )
    connected_binary = output / "connected"
    subprocess.run(
        [
            "clang++",
            "-std=c++17",
            "-O2",
            str(connected_cpp),
            "-o",
            str(connected_binary),
        ],
        check=True,
    )
    connected = subprocess.run(
        [str(connected_binary)], capture_output=True, text=True, check=True
    ).stdout
    connected_groups: dict[int, list[tuple[int, int, int]]] = {i: [] for i in range(9)}
    for row in csv.DictReader(io.StringIO(connected)):
        connected_groups[int(row["case"])].append(
            (int(row["h"]), int(row["plane"]), int(row["mod"]))
        )
    if (
        any(connected_groups[i] for i in (0, 1, 3))
        or connected_groups[2][:4] != [(74, 3, 0), (75, 1, 0), (76, 2, 0), (77, 0, 0)]
        or any(
            connected_groups[i][:3] != [(65, 0, 0), (73, 0, 0), (81, 0, 0)]
            for i in (4, 5)
        )
        or any(connected_groups[i] != [(65, 0, 0), (73, 0, 1)] for i in (6, 8))
        or connected_groups[7] != [(65, 0, 0), (73, 0, 0), (81, 0, 1)]
    ):
        raise SystemExit("connected-case comparator coverage differs")
    if source_path.read_bytes() != original:
        raise SystemExit("reference source changed during probe")
    report = {
        "source_sha256": SOURCE_SHA256,
        "fragment_sha256": {
            name: digest(value.encode()) for name, value in fragments.items()
        },
        "probe_sha256": digest((ROOT / "probe.cpp.in").read_bytes()),
        "cases": len(cases),
        "requests": len(events),
        "static_controls": len(controls),
        "successive_write_queue_checks": 3,
        "immediate_write_mismatching_cases": len(differing),
        "cases_sha256": digest((header + expected.stderr).encode()),
        "events_sha256": digest(expected.stdout.encode()),
        "connected_cases": 9,
        "connected_probe_sha256": digest((ROOT / "connected.cpp.in").read_bytes()),
        "connected_events_sha256": digest(connected.encode()),
        "scope": "Compiled reference register handlers and reservation sequencer; external write phase; no CPU pin timing, RGA arbitration or pixels",
    }
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    if immediate:
        raise SystemExit(
            f"immediate-write control failed: {len(differing)} cases differ"
        )
    (output / "registered-cases.csv").write_text(header + expected.stderr)
    (output / "registered-events.csv").write_text(expected.stdout)
    (output / "registered-connected-events.csv").write_text(connected)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument(
        "--immediate", action="store_true", help="negative control; must fail"
    )
    args = parser.parse_args()
    build(args.source, args.output, args.immediate)

#!/usr/bin/env python3
"""Run the exact latch regressions with an interval-predicate negative control."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[4]
base = Path(sys.argv[1]).resolve()
base.mkdir(parents=True, exist_ok=True)
source_path = repo / "crates/common-commodore-amiga/src/denise.rs"
source = source_path.read_text()


def function(name: str) -> str:
    start = source.index("fn " + name + "(")
    brace = source.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


function_text = function("horizontal_diw_active")
tests = "\n".join(
    "#[test]\n" + function(name)
    for name in [
        "horizontal_diw_gate_obeys_the_variant_comparator_phase",
        "horizontal_diw_retains_only_matches_across_counter_reset_and_register_rewrites",
    ]
)
prefix = "#[derive(Clone, Copy)]\nenum HorizontalDiwComparatorPhase { BeforeOutput, AfterOutput }\n"
mutant = function_text.replace(
    "if beam_x_lores == hstart {\n        *active = true;\n    }\n    if beam_x_lores == hstop {\n        *active = false;\n    }",
    "*active = beam_x_lores >= hstart && beam_x_lores < hstop;",
)
assert mutant != function_text
results = {}
for name, implementation in [("latch", function_text), ("interval-mutant", mutant)]:
    path = base / (name + ".rs")
    path.write_text(prefix + implementation + "\n" + tests)
    binary = base / name
    subprocess.run(
        ["rustc", "--edition=2024", "--test", str(path), "-o", str(binary)], check=True
    )
    result = subprocess.run([str(binary)], capture_output=True, text=True, check=False)
    (base / (name + ".log")).write_text(result.stdout + result.stderr)
    results[name] = {
        "exit_code": result.returncode,
        "harness_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }
assert results["latch"]["exit_code"] == 0
assert results["interval-mutant"]["exit_code"] != 0
assert (
    "counter reset is not a window-stop event"
    in (base / "interval-mutant.log").read_text()
)
results["source_sha256"] = hashlib.sha256(source_path.read_bytes()).hexdigest()
(base / "latch-negative-control.json").write_text(json.dumps(results, indent=2) + "\n")
print(json.dumps(results, indent=2))

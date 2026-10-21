#!/usr/bin/env python3
"""Run a temporary, deliberately red controller probe without changing library code."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--output", required=True, type=Path)
parser.add_argument("--target-dir", required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
probe = Path(__file__).with_name("probe.rs")
installed = root / "crates/nec-upd765a/tests/read_data_signals_probe.rs"
args.output.mkdir(parents=True, exist_ok=False)
if installed.exists():
    raise SystemExit(f"refuse to replace {installed}")
command = [
    "cargo",
    "+1.99.0",
    "test",
    "--locked",
    "-p",
    "nec-upd765a",
    "--test",
    "read_data_signals_probe",
    "--",
    "--nocapture",
]
environment = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()))
started = time.monotonic()
try:
    shutil.copyfile(probe, installed)
    completed = subprocess.run(
        command,
        cwd=root,
        env=environment,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
finally:
    installed.unlink(missing_ok=True)
elapsed = time.monotonic() - started
(args.output / "stdout.log").write_text(completed.stdout)
(args.output / "stderr.log").write_text(completed.stderr)
executable_match = re.search(r"Running .*? \(([^\n]+)\)", completed.stderr)
if executable_match is None:
    raise SystemExit("Cargo did not report an actual test executable")
executable = Path(executable_match[1])
receipt = {
    "schema_version": 1,
    "qualified": False,
    "kind": "fail-first-investigation",
    "source_commit": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip(),
    "argv": command,
    "relevant_environment": {"CARGO_TARGET_DIR": environment["CARGO_TARGET_DIR"]},
    "host_timeout_seconds": 120,
    "elapsed_seconds": elapsed,
    "returncode": completed.returncode,
    "probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(),
    "test_executable": str(executable),
    "test_executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
    "library_source_sha256": hashlib.sha256(
        (root / "crates/nec-upd765a/src/lib.rs").read_bytes()
    ).hexdigest(),
    "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
    "rustc": subprocess.check_output(["rustc", "+1.99.0", "-Vv"], text=True).strip(),
    "cargo": subprocess.check_output(["cargo", "+1.99.0", "-V"], text=True).strip(),
}
(args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(completed.stdout)
print(completed.stderr)
print(json.dumps(receipt, indent=2))
if (
    completed.returncode != 101
    or "3 passed; 4 failed; 0 ignored" not in completed.stdout
):
    raise SystemExit(
        "probe did not produce the expected bounded 3-pass/4-fail current-code result"
    )

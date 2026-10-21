#!/usr/bin/env python3
"""Audit this captured test packet; never issue an operation qualification."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


def require(value: object, message: str) -> None:
    if not value:
        raise ValueError(message)


def expected_cases() -> dict[str, tuple[str, list[int]]]:
    expected = {}
    for opcode in ("0d", "4d"):
        for nd in ("false", "true"):
            for action in ("success", "protect", "overrun", "reset"):
                size = 9 if action == "success" else 8
                status = {
                    "success": [0, 0, 0],
                    "protect": [0x40, 2, 0],
                    "overrun": [0x40, 0x10, 0],
                    "reset": [],
                }[action]
                for accepted in range(size):
                    for rqm in ("false", "true"):
                        expected[f"{opcode}/{nd}/{action}/{accepted}/{rqm}"] = (
                            action,
                            status,
                        )
            for writable in ("false", "true"):
                for drained in range(8):
                    expected[
                        f"{opcode}/{nd}/initial-protection/{writable}/{drained}"
                    ] = ("initial-protection", [0x40, 2, 0])
    require(len(expected) == 328, "auditor case definition")
    return expected


def check_cases(record: dict) -> list[dict]:
    require(record["qualified"] is False, "no operation verdict")
    require(record["snapshot_version"] == 10, "snapshot identity")
    expected = expected_cases()
    cases = record["cases"]
    require(record["case_count"] == len(cases) == 328, "nonzero exact case count")
    require(len({case["id"] for case in cases}) == 328, "duplicate cases")
    require({case["id"] for case in cases} == set(expected), "case coverage")
    for case in cases:
        fields = case["id"].split("/")
        require(case["opcode"] == int(fields[0], 16), "opcode identity")
        require(case["nd"] is (fields[1] == "true"), "ND identity")
        if fields[2] == "initial-protection":
            require(case["writable"] is (fields[3] == "true"), "mount identity")
            require(case["drained"] == int(fields[4]), "result boundary identity")
        else:
            require(case["accepted"] == int(fields[3]), "prefix identity")
            require(case["rqm_high"] is (fields[4] == "true"), "RQM identity")
        action, status = expected[case["id"]]
        require(case["action"] == action, "case action identity")
        require(case["status_expected"] == status, "independent expected status")
        result = case["result_observed"]
        require(
            isinstance(result, list)
            and all(type(byte) is int and 0 <= byte <= 255 for byte in result),
            "result bytes",
        )
        if action == "reset":
            require(result == [], "reset has no command result")
        else:
            require(len(result) == 7 and result[:3] == status, "observed status")
        if action == "success":
            require(result == [0, 0, 0, 4, 5, 10, 0], "observed success CHRN")
        for field in ("native_equal", "source_unchanged", "export_reopened"):
            require(case[field] is True, field)
    return cases


def audit(directory: Path) -> dict:
    verification = json.loads((directory / "verification.json").read_text())
    require(verification["qualified"] is False, "packet cannot qualify operation")
    required = {
        "wrong-result.log",
        "idle-checkpoint.log",
        "positive-1.log",
        "positive-2.log",
        "owning-suites.log",
        "fmt.log",
        "included-file-fmt.log",
        "clippy.log",
        "run-1/format-native.json",
        "run-2/format-native.json",
        "verify.py",
    }
    require(set(verification["artifact_hashes"]) == required, "artifact coverage")
    for name, digest in verification["artifact_hashes"].items():
        require(
            hashlib.sha256((directory / name).read_bytes()).hexdigest() == digest,
            f"artifact hash: {name}",
        )
    commands = {row["log"]: row for row in verification["commands"]}
    require(len(commands) == len(verification["commands"]) == 8, "command coverage")
    for name in {name for name in required if name.endswith(".log")}:
        row = commands[name]
        require(row["host_timeout_seconds"] == 300, "host bound")
        wanted = 101 if name in ("wrong-result.log", "idle-checkpoint.log") else 0
        require(row["exit_code"] == wanted, f"command outcome: {name}")
    for name, reason in (
        ("wrong-result.log", "Format success result"),
        ("idle-checkpoint.log", "Format checkpoint fixed point"),
    ):
        log = (directory / name).read_text()
        require(reason in log and "1 failed" in log, f"negative reason: {name}")
    results = []
    for number in (1, 2):
        log = (directory / f"positive-{number}.log").read_text()
        require(
            "FORMAT_NATIVE_CASES=328" in log and "1 passed; 0 failed; 0 ignored" in log,
            "positive executed count",
        )
        results.append(
            check_cases(
                json.loads((directory / f"run-{number}/format-native.json").read_text())
            )
        )
    require(results[0] == results[1], "repeat observations agree")
    return {
        "packet_checks": "pass",
        "cases_per_run": 328,
        "runs": 2,
        "qualified": False,
    }


if __name__ == "__main__":
    print(json.dumps(audit(Path(__file__).resolve().parent), indent=2))

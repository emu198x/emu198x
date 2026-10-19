#!/usr/bin/env python3
"""Audit retained source-test evidence; does not execute an emulator."""

from __future__ import annotations

import hashlib
import json
import re
import tarfile
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def main() -> None:
    here = Path(__file__).resolve().parent
    repo = here.parents[2]
    record = json.loads((here / "verification.json").read_text())
    require(
        record["qualified"] is False, "controller evidence cannot qualify a machine"
    )
    for relative, expected in record["source_sha256"].items():
        require(
            hashlib.sha256((repo / relative).read_bytes()).hexdigest() == expected,
            f"source identity differs: {relative}",
        )
    require(len(record["checks"]) == 4, "four positive commands are required")
    require(record["red_control"]["exit_code"] == 101, "red control must fail")
    archive = here / "checks.tar.gz"
    require(
        hashlib.sha256(archive.read_bytes()).hexdigest() == record["archive_sha256"],
        "raw log archive differs",
    )
    with tarfile.open(archive) as tar:

        def log(name: str) -> str:
            stream = tar.extractfile(name)
            require(stream is not None, f"missing log: {name}")
            if stream is None:
                raise ValueError(name)
            return stream.read().decode()

        red = log("red.log")
        require(
            "1 failed" in red
            and "0 passed" in red
            and "left: [0, 0, 0, 0, 0, 7, 2]" in red
            and "right: [64, 36, 0, 0, 0, 7, 2]" in red,
            "red control must reject the original false success",
        )
        require(
            "1 passed; 0 failed" in log("green.log"), "focused green did not execute"
        )
        tests = log("tests.log")
        counts = re.findall(
            r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", tests
        )
        require(
            sum(int(row[0]) for row in counts) == 141
            and all(row[1:] == ("0", "0") for row in counts),
            "controller tests must execute all 141 without failure/ignore",
        )
        require("Finished `dev` profile" in log("clippy.log"), "Clippy did not finish")
        require(
            all(check["exit_code"] == 0 for check in record["checks"]),
            "positive command failed",
        )
    print(
        "Read ID source evidence: PASS (141 controller tests; one rejecting red control)"
    )


if __name__ == "__main__":
    main()

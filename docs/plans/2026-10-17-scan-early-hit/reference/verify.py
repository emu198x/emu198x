#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Verify retained controller evidence, without executing a reference or ROM."""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
EVIDENCE = HERE.parent / "evidence"
PINS = json.loads((EVIDENCE / "identities.json").read_text())
CASES = ["first-hit", "later-hit", "no-hit", "bad-crc"]
HOSTS = [33, 34, 35, 33]


def require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def crc(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ (0x1021 if value & 0x8000 else 0)) & 0xFFFF
    return value


def check(record: dict, root: Path) -> dict:
    require(
        record["schema_version"] == 1
        and record["qualified"] is False
        and record["kind"]
        == "controller-register-probes-not-rom-or-hardware-qualification"
        and record["evidence_class"] == "fresh_execution",
        "false qualification or route",
    )
    for key in [
        "source_commit",
        "sources",
        "fuse_source",
        "mame_identity",
        "emu_identity",
        "firmware_sha256",
        "compiler",
    ]:
        require(record[key] == PINS[key], "changed identity: " + key)
    hashes = 0
    require(len(record["artifacts"]) == 88, "missing artifacts")
    for item in record["artifacts"]:
        path = Path(item["path"])
        require(
            not path.is_absolute()
            and len(path.parts) == 1
            and path.suffix not in {".rom", ".state", ".sna", ".z80"},
            "unsafe artifact",
        )
        data = (root / path).read_bytes()
        require(
            len(data) == item["size"] and sha(data) == item["sha256"],
            "changed artifact",
        )
        hashes += 1
    require(
        record["commands"]
        and all(
            c["returncode"] == 0 and 0 <= c["host_seconds"] <= c["timeout_seconds"]
            for c in record["commands"]
        ),
        "failed or unbounded command",
    )
    require(
        len({c["name"] for c in record["commands"]}) == len(record["commands"]),
        "duplicate commands",
    )
    require(len(record["fuse"]) == 16, "missing Fuse observations")
    require(
        {(r["runtime"], r["repeat"], r["case"]) for r in record["fuse"]}
        == {
            (runtime, repeat, case)
            for runtime in ["stock", "patched"]
            for repeat in [1, 2]
            for case in CASES
        },
        "duplicate Fuse case",
    )
    raw_good = None
    for row in record["fuse"]:
        label, case, repeat = row["runtime"], row["case"], row["repeat"]
        require(
            row["identity"] == PINS["fuse_binaries"][label],
            "wrong stock/patched identity",
        )
        obs = row["observation"]
        expected_count = (
            512 if label == "stock" or case in {"first-hit", "bad-crc"} else 1024
        )
        require(
            obs["host"] == HOSTS[CASES.index(case)] and obs["sent"] == expected_count,
            "wrong Fuse byte count",
        )
        expected = {
            "stock": [
                [0, 0, 8, 0, 0, 2, 2],
                [64, 128, 4, 0, 0, 2, 2],
                [64, 128, 4, 0, 0, 2, 2],
                [0, 32, 40, 0, 0, 2, 2],
            ],
            "patched": [
                [0, 0, 8, 0, 0, 2, 2],
                [0, 0, 8, 0, 0, 3, 2],
                [64, 128, 4, 0, 0, 2, 2],
                [0, 32, 40, 0, 0, 2, 2],
            ],
        }[label][CASES.index(case)]
        require(
            obs["result"] == expected
            and obs["idle_msr"] == 128
            and obs["media_unchanged"] is True
            and 0 < obs["tstates"] < 2 * 3546900,
            "wrong Fuse result or media mutation",
        )
        name = f"fuse-{label}-{repeat}-{case}"
        require(not (root / (name + ".stderr")).read_bytes(), "sanitizer diagnostic")
        require(
            json.loads((root / (name + ".stdout")).read_text()) == obs,
            "invented Fuse observation",
        )
        track = (root / row["track"]).read_bytes()
        require(
            sha(track) == row["track_sha256"]
            and len(track) == 8599
            and track[:3] == bytes([106, 24, 0]),
            "wrong raw track",
        )
        data = track[3:6253]
        require(
            data[140:652] == bytes([33]) * 512 and data[756:1268] == bytes([34]) * 512,
            "wrong fixture payloads",
        )
        require(
            crc(data[92:102]) == crc(data[708:718]) == crc(data[752:1270]) == 0,
            "invalid fixture field CRC",
        )
        require(
            (crc(data[136:654]) != 0) is (case == "bad-crc"),
            "missing CRC negative control",
        )
        if case != "bad-crc":
            if raw_good is None:
                raw_good = track
            require(track == raw_good, "different normal fixture")
        else:
            require(
                raw_good is not None
                and [
                    i
                    for i, (a, b) in enumerate(zip(track, raw_good, strict=True))
                    if a != b
                ]
                == [655],
                "wrong deliberate CRC corruption",
            )
    require(
        len(record["mame"]) == 6
        and {(r["repeat"], r["case"]) for r in record["mame"]}
        == {(repeat, case) for repeat in [1, 2] for case in CASES[:3]},
        "missing/duplicate MAME cases",
    )
    for row in record["mame"]:
        obs = row["observation"]
        actual = obs["observed"]
        flags = 8 if row["case"] == "later-hit" else 4
        require(
            actual
            == {
                "sent": 1024,
                "data": [],
                "result": [64, 128, flags, 0, 0, 2, 2],
                "idle_msr": 128,
            }
            and 0 < obs["seconds"] <= 2
            and row["media_unchanged"] is True,
            "wrong stock MAME observation",
        )
    require(
        len(record["emu"]) == 2 and {r["repeat"] for r in record["emu"]} == {1, 2},
        "missing Emu repeat",
    )
    for row in record["emu"]:
        require(
            row["observations"]
            == [
                {
                    "case": "first-hit",
                    "host": 33,
                    "sent": 512,
                    "result": [0, 0, 8, 0, 0, 1, 2],
                    "media_unchanged": True,
                },
                {
                    "case": "later-hit",
                    "host": 34,
                    "sent": 1024,
                    "result": [0, 0, 8, 0, 0, 2, 2],
                    "media_unchanged": True,
                },
                {
                    "case": "no-hit",
                    "host": 35,
                    "sent": 1024,
                    "result": [64, 128, 4, 0, 0, 2, 2],
                    "media_unchanged": True,
                },
            ],
            "wrong Emu transfer/result",
        )
        text = (root / f"emu-{row['repeat']}.stdout").read_text()
        require(
            [
                json.loads(line.split("SCAN_OBSERVATION ", 1)[1])
                for line in text.splitlines()
                if "SCAN_OBSERVATION " in line
            ]
            == row["observations"]
            and "1 passed; 0 failed; 0 ignored" in text,
            "zero/unexecuted Emu test",
        )
    tests = (root / "controller-tests.stdout").read_text()
    require(
        sum(int(n) for n in re.findall(r"test result: ok\. (\d+) passed;", tests))
        == 108
        and "FAILED" not in tests,
        "wrong controller suite result",
    )
    return {
        "qualified": False,
        "scope": "first-hit transfer count only; full Scan conformance remains open",
        "fresh_observations": 28,
        "artifact_hashes": hashes,
        "first_hit_counts": {
            "Emu source test": 512,
            "unmodified Fuse source controller": 512,
            "installed MAME": 1024,
        },
        "stock_reference_count_failures": 6,
        "experimental_patch": "separate; not upstream release or hardware validation",
        "result_chrn": "disagrees; unqualified",
    }


def main(root: Path | None = None) -> dict:
    if root is not None:
        return check(json.loads((root / "campaign.json").read_text()), root)
    index = json.loads((EVIDENCE / "hashes.json").read_text())
    archive = EVIDENCE / "runs.tar.gz"
    require(sha(archive.read_bytes()) == index["archive_sha256"], "archive changed")
    with tempfile.TemporaryDirectory(prefix="198x-scan-audit-") as temp:
        root = Path(temp)
        with tarfile.open(archive) as bundle:
            members = bundle.getmembers()
            require(
                len(members) == 89
                and all(m.isfile() for m in members)
                and {m.name for m in members} == {r["path"] for r in index["files"]},
                "unsafe/incomplete archive",
            )
            bundle.extractall(root, filter="data")
        for item in index["files"]:
            data = (root / item["path"]).read_bytes()
            require(
                len(data) == item["size"] and sha(data) == item["sha256"],
                "archive artifact changed",
            )
        return main(root)


if __name__ == "__main__":
    print(json.dumps(main(Path(sys.argv[1]) if len(sys.argv) > 1 else None), indent=2))

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Audit fresh MFI reference evidence and explicitly replay the prior baseline."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import tarfile
import tempfile
from pathlib import Path

from decode import validate

HERE = Path(__file__).resolve().parent
PRIOR = HERE.parent / "2026-10-19-read-track-drained-crc"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def audit(root: Path) -> dict:
    r = json.loads((root / "campaign.json").read_text())
    require(
        r["schema_version"] == 1 and r["completed"] and not r["trial"],
        "incomplete full campaign",
    )
    require(
        r["qualified"] is False
        and r["evidence_class"] == "fresh_reference_execution_with_captured_baseline",
        "false qualification or evidence class",
    )
    require(r["private_copies_removed"] is True, "private firmware cleanup incomplete")
    require(
        r["baseline"]["kind"] == "captured_reference_replay"
        and r["baseline"]["emu_fuse_fresh_this_study"] is False,
        "baseline replay promoted to fresh",
    )
    require(
        sha(PRIOR / "evidence.tar.gz") == r["baseline"]["sha256"],
        "different baseline archive",
    )
    for rel, digest in r["source"]["authored"].items():
        require(sha(HERE / rel) == digest, "different capture adapter")
    hashes = json.loads((root / "hashes.json").read_text())
    require(
        bool(hashes)
        and set(hashes)
        == {p.name for p in root.iterdir() if p.is_file() and p.name != "hashes.json"},
        "missing raw artifacts",
    )
    for rel, digest in hashes.items():
        require(sha(root / rel) == digest, "raw artifact hash mismatch")
    spec = importlib.util.spec_from_file_location(
        "raw_baseline_auditor", PRIOR / "verify.py"
    )
    require(spec is not None and spec.loader is not None, "missing baseline auditor")
    old = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(old)
    with tempfile.TemporaryDirectory(prefix="198x-raw-baseline-audit-") as temp:
        oldroot = Path(temp)
        with tarfile.open(PRIOR / "evidence.tar.gz") as archive:
            archive.extractall(oldroot, filter="data")
        require(old.audit(oldroot) == r["baseline"]["audit"], "invalid baseline replay")
        original = json.loads((oldroot / "campaign.json").read_text())
        expected_replay = [
            x for x in original["observations"] if x["engine"] in ("emu", "fuse")
        ]
        require(
            r["replayed_observations"] == expected_replay, "changed replay observations"
        )
        require(
            r["mame"]["sha256"] == original["mame_executable"]["sha256"]
            and r["mame"]["source_commit"] == "unknown"
            and r["mame"]["delivery"] == "installed_binary"
            and r["inspected_mame_source"]["executed_source_identity"] is False,
            "invented reference identity",
        )
        require(
            r["firmware_sha256"] == original["firmware_sha256"], "different firmware"
        )
        baseline = {
            (x["engine"], x["repeat"], x["order"], x["kind"], x["position"]): x
            for x in expected_replay
        }
        cases = {
            (order, kind, pos)
            for order in range(3)
            for kind, pos in [(0, 0), (1, 0), (2, 0), (1, 1), (2, 1)]
            if not (order == 0 and pos == 1)
        }
        fixtures = r["fixtures"]
        keys = [(x["order"], x["kind"], x["position"]) for x in fixtures]
        require(len(keys) == 13 and set(keys) == cases, "missing or duplicate fixtures")
        indexed = dict(zip(keys, fixtures, strict=True))
        for key, f in indexed.items():
            source = baseline[("fuse", 1, *key)]
            raw = root / f["raw"]
            media = root / f["media"]
            require(
                raw.read_bytes() == (oldroot / source["media"]).read_bytes()
                and sha(raw)
                == f["raw_sha256"]
                == f["baseline_raw_sha256"]
                == source["media_sha256"],
                "different audited raw fixture",
            )
            require(sha(media) == f["media_sha256"], "changed MFI input")
            require(
                validate(media.read_bytes(), raw.read_bytes()) == f["validation"],
                "invalid cell validation",
            )
    rows = r["observations"]
    expected = {
        (op, repeat, order, kind, pos)
        for repeat in (1, 2)
        for op, order, kind, pos in [
            *[("track", *c) for c in cases],
            *[(op, 0, kind, 0) for op in ("id", "data") for kind in range(3)],
        ]
    }
    keys = [
        (x["operation"], x["repeat"], x["order"], x["kind"], x["position"])
        for x in rows
    ]
    require(
        len(keys) == 38 and set(keys) == expected, "missing or duplicate observations"
    )
    observations = dict(zip(keys, rows, strict=True))
    comparisons = []
    require(
        len(r["commands"]) == 40 and len({c["name"] for c in r["commands"]}) == 40,
        "missing invocation",
    )
    names = {c["name"] for c in r["commands"]}
    for command in r["commands"]:
        require(
            not command["timed_out"]
            and command["returncode"] == 0
            and 0 < command["host_seconds"] < command["host_timeout_seconds"] <= 45,
            "failed or unbounded invocation",
        )
        require(
            all(
                command["name"] + "." + suffix in hashes
                for suffix in ("stdout", "stderr")
            ),
            "missing process logs",
        )
    for key, row in observations.items():
        op, repeat, order, kind, pos = key
        data = row["observed"]
        require(
            json.loads((root / (row["name"] + ".json")).read_text()) == data,
            "observation differs from raw output",
        )
        command = next(c for c in r["commands"] if c["name"] == row["name"])
        argv = command["argv"]
        require(
            row["opcode"] == {"track": 0x42, "id": 0x4A, "data": 0x46}[op]
            and command["environment"]["PROBE_OPCODE"] == str(row["opcode"])
            and command["environment"]["PROBE_BYTES"]
            == str(row["expected_payload_bytes"])
            and command["environment"]["PROBE_COUNT"] == str(1 if order == 0 else 2)
            and Path(argv[argv.index("-flop1") + 1]).name == row["media"]
            and argv[1] == "specpls3"
            and argv[argv.index("-bios") + 1] == "en"
            and argv[argv.index("-seconds_to_run") + 1] == "10",
            "different probe invocation",
        )
        fixture = indexed[(order, kind, pos)]
        require(
            row["name"] in names
            and row["media"] == fixture["media"]
            and row["media_sha256"] == fixture["media_sha256"]
            and row["media_unchanged"] is True,
            "wrong invocation or mutated source",
        )
        payload = (
            []
            if op == "id" or (op == "data" and kind == 1)
            else [0x20 + x for x in [[1], [1, 2], [2, 1]][order] for _ in range(512)]
        )
        require(
            data["data"] == payload and row["expected_payload_bytes"] == len(payload),
            "incomplete or wrong payload",
        )
        require(
            len(data["result"]) == 7
            and all(type(b) is int and 0 <= b <= 255 for b in data["result"]),
            "incomplete result",
        )
        require(
            data["idle_msr"] == 128
            and data["recovery_msr"] == 128
            and data["recovery"] == 0x30,
            "missing next-command recovery",
        )
        require(0 < data["seconds"] < 2, "emulated bound exceeded")
        other = observations[(op, 3 - repeat, order, kind, pos)]["observed"]
        require(
            all(
                data[k] == other[k]
                for k in ("data", "result", "idle_msr", "recovery", "recovery_msr")
            ),
            "nonrepeatable observation",
        )
        if op == "track":
            replay = baseline[("emu", repeat, order, kind, pos)]["observed"]
            require(
                data["result"] == replay["result"],
                "Read Track differs from recorded Emu",
            )
            if repeat == 1:
                comparisons.append(
                    {
                        "order": order,
                        "kind": kind,
                        "position": pos,
                        "mame_fresh": data["result"],
                        "emu_replay": replay["result"],
                        "fuse_replay": baseline[("fuse", repeat, order, kind, pos)][
                            "observed"
                        ]["result"],
                        "mame_emu_seven_bytes_equal": True,
                    }
                )
        elif op == "id":
            expected_status = (
                [64, 37, 0, 0, 0, 1, 2] if kind == 1 else [0, 0, 0, 0, 0, 1, 2]
            )
            require(
                data["result"] == expected_status, "ID-only CRC control not observed"
            )
        else:
            expected_status = (
                [64, 128, 0, 0, 0, 1, 2]
                if kind == 0
                else [64, 32, 32 if kind == 2 else 0, 0, 0, 1, 2]
            )
            require(
                data["result"] == expected_status, "Read Data CRC control not observed"
            )
    return {
        "schema_version": 1,
        "qualified": False,
        "fresh_mame_observations": 38,
        "replayed_emu_fuse_observations": 52,
        "read_track_cases": 13,
        "repeats": 2,
        "comparisons": sorted(
            comparisons, key=lambda x: (x["order"], x["kind"], x["position"])
        ),
        "id_only_control": "pass",
        "raw_cell_clock_crc_preservation": "pass",
        "machine_qualification": "not_run",
        "hardware_timing": "not_run",
        "fuse_disagreements": "retained_not_adjudicated",
    }


def main() -> None:
    if len(sys.argv) > 1:
        result = audit(Path(sys.argv[1]))
    else:
        with tempfile.TemporaryDirectory(prefix="198x-raw-mfi-audit-") as temp:
            with tarfile.open(HERE / "evidence.tar.gz") as archive:
                archive.extractall(temp, filter="data")
            result = audit(Path(temp))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

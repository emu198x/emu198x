#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Audit captured observations and independently validate raw CRC controls.

Pinned results are observations, not a hardware conformance oracle. No emulator
is executed by this auditor. MAME's DSK ID-only rows cannot establish ID errors.
"""

from __future__ import annotations

import hashlib
import json
import sys
import tarfile
import tempfile
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
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


def audit(root: Path) -> dict:
    record = json.loads((root / "campaign.json").read_text())
    require(
        record["schema_version"] == 1 and record["completed"], "incomplete campaign"
    )
    require(
        record["qualified"] is False and record["evidence_class"] == "fresh_execution",
        "false qualification or replay",
    )
    here = Path(__file__).resolve().parent
    repo = here.parents[2]
    for relative, digest in record["authored_sources"].items():
        require(
            sha((repo / relative).read_bytes()) == digest, "different authored adapter"
        )
    require(
        sha((repo / "crates/nec-upd765a/tests/read_track_drained.rs").read_bytes())
        == record["emu_source"]["test_sha256"],
        "different Emu observation probe",
    )
    hashes = json.loads((root / "hashes.json").read_text())
    require(
        bool(hashes)
        and set(hashes)
        == {p.name for p in root.iterdir() if p.is_file() and p.name != "hashes.json"},
        "missing artifact manifest",
    )
    for name, digest in hashes.items():
        require(sha((root / name).read_bytes()) == digest, "artifact hash mismatch")
    require(
        record["fuse_source"]["patched"] is False
        and record["fuse_source"]["commit"]
        == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
        and record["fuse_source"]["files"]["peripherals/disk/upd_fdc.c"]
        == "845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d",
        "patched or different Fuse",
    )
    for role in ("emu_executable", "fuse_executable", "mame_executable"):
        require(len(record[role]["sha256"]) == 64, "missing executable identity")
    require(
        record["mame_executable"]["source_commit"] == "unknown"
        and record["mame_executable"]["delivery"] == "installed_binary"
        and record["mame_inspected_source"]["executed_source_identity"] is False,
        "invented MAME source identity",
    )
    cases = {
        (order, kind, pos)
        for order in range(3)
        for kind, pos in [(0, 0), (1, 0), (2, 0), (1, 1), (2, 1)]
        if not (order == 0 and pos == 1)
    }
    expected = {
        (engine, repeat, *case)
        for engine in ("emu", "fuse", "mame")
        for repeat in (1, 2)
        for case in cases
    }
    observations = record["observations"]
    keys = [
        (x["engine"], x["repeat"], x["order"], x["kind"], x["position"])
        for x in observations
    ]
    require(
        len(keys) == len(expected) and set(keys) == expected,
        "missing or duplicate observations",
    )
    indexed = dict(zip(keys, observations, strict=True))
    require(
        len(record["commands"]) == 58
        and len({c["name"] for c in record["commands"]}) == 58,
        "missing commands",
    )
    for command in record["commands"]:
        require(
            not command["timed_out"]
            and command["returncode"] == 0
            and 0 < command["host_seconds"] < command["timeout_seconds"] <= 180,
            "failed or unbounded command",
        )
        for suffix in ("stdout", "stderr"):
            require(f"{command['name']}.{suffix}" in hashes, "missing command log")
    comparisons = []
    for key, row in indexed.items():
        engine, repeat, order, kind, position = key
        ids = [[1], [1, 2], [2, 1]][order]
        n = len(ids)
        obs = row["observed"]
        payload = [0x20 + sector for sector in ids for _ in range(512)]
        require(obs["data"] == payload, "incomplete or wrong-order payload")
        require(
            len(obs["result"]) == 7
            and all(type(b) is int and 0 <= b <= 255 for b in obs["result"]),
            "incomplete seven-byte result",
        )
        require(
            obs["idle_msr"] == 128
            and obs["recovery_msr"] == 128
            and 0 <= obs["recovery"] <= 255,
            "missing next-command recovery",
        )
        require(obs["media_unchanged"] is True, "changed source medium")
        if engine == "emu":
            require(0 < obs["ticks"] < 2_000_000, "unbounded Emu ticks")
        if engine == "fuse":
            require(0 < obs["tstates"] < 7_093_800, "unbounded Fuse ticks")
        if engine == "mame":
            require(0 < obs["seconds"] < 2, "unbounded MAME time")
        other = indexed[(engine, 3 - repeat, order, kind, position)]["observed"]
        require(
            all(
                obs[k] == other[k]
                for k in ("data", "result", "idle_msr", "recovery", "recovery_msr")
            ),
            "nonrepeatable observation",
        )
        # Bind retained results without claiming these values are silicon truth.
        if engine in ("emu", "mame"):
            status = [
                64,
                128 | (4 if ids[-1] != 1 else 0) | (32 if kind else 0),
                32 if kind and (engine == "mame" or kind == 2) else 0,
                0,
                0,
                1,
                2,
            ]
        else:
            status = [
                0,
                (4 if ids[-1] != n else 0)
                | (
                    36
                    if kind == 1 and position == n - 1
                    else 32
                    if kind == 2 and position == n - 1
                    else 0
                ),
                32 if kind == 2 else 0,
                0,
                0,
                1 + n,
                2,
            ]
        require(obs["result"] == status, "changed captured status observation")
        if engine in ("fuse", "mame"):
            media = (root / row["media"]).read_bytes()
            require(sha(media) == row["media_sha256"], "wrong media identity")
            if engine == "mame":
                require(
                    len(media) == 512 + 512 * n and media[48:50] == bytes([1, 1]),
                    "wrong DSK layout",
                )
                for i, sector in enumerate(ids):
                    require(
                        media[280 + 8 * i : 288 + 8 * i]
                        == bytes(
                            [
                                0,
                                0,
                                sector,
                                2,
                                32 if kind and position == i else 0,
                                32 if kind == 2 and position == i else 0,
                                0,
                                2,
                            ]
                        ),
                        "wrong DSK metadata",
                    )
                    require(
                        media[512 + 512 * i : 1024 + 512 * i]
                        == bytes([0x20 + sector]) * 512,
                        "wrong DSK payload",
                    )
            else:
                require(len(media) == 8599, "wrong raw layout")
                track = media[3:6253]
                clocks = media[6253:7035]
                marked = {i for i in range(6250) if clocks[i // 8] & (1 << (i % 8))}
                expected_marks = set()
                for i, sector in enumerate(ids):
                    start = 92 + 616 * i
                    data_start = start + 44
                    expected_marks.update(range(start, start + 3))
                    expected_marks.update(range(data_start, data_start + 3))
                    require(
                        track[start : start + 8]
                        == bytes([161, 161, 161, 254, 0, 0, sector, 2]),
                        "wrong raw ID",
                    )
                    require(
                        track[data_start : data_start + 516]
                        == bytes([161, 161, 161, 251]) + bytes([0x20 + sector]) * 512,
                        "wrong raw payload",
                    )
                    require(
                        (crc(track[start : start + 10]) != 0)
                        == (kind == 1 and position == i),
                        "wrong independent ID CRC",
                    )
                    require(
                        (crc(track[data_start : data_start + 518]) != 0)
                        == (kind == 2 and position == i),
                        "wrong independent data CRC",
                    )
                require(marked == expected_marks, "wrong missing-clock marks")
                clean = indexed[("fuse", repeat, order, 0, 0)]
                original = (root / clean["media"]).read_bytes()
                changed = [
                    i
                    for i, (a, b) in enumerate(zip(original, media, strict=True))
                    if a != b
                ]
                intended = (
                    []
                    if kind == 0
                    else [3 + 92 + 616 * position + (8 if kind == 1 else 560)]
                )
                require(
                    changed == intended
                    and all(original[i] ^ media[i] == 1 for i in changed),
                    "wrong isolated CRC mutation",
                )
        if repeat == 1 and engine == "emu":
            fuse = indexed[("fuse", 1, order, kind, position)]["observed"]["result"]
            mame = indexed[("mame", 1, order, kind, position)]["observed"]["result"]
            comparisons.append(
                {
                    "order": order,
                    "kind": kind,
                    "position": position,
                    "emu": obs["result"],
                    "fuse": fuse,
                    "mame": mame,
                    "mame_id_crc_comparable": kind != 1,
                    "emu_mame_full_equal": obs["result"] == mame,
                    "emu_fuse_full_equal": obs["result"] == fuse,
                }
            )
    return {
        "schema_version": 1,
        "qualified": False,
        "observations": len(indexed),
        "cases": len(cases),
        "repeats": 2,
        "payload_and_recovery_checks": True,
        "comparisons": comparisons,
        "mame_id_crc": "not_qualified_fixture_translation",
        "hardware_timing": "not_run",
    }


def main() -> None:
    if len(sys.argv) > 1:
        result = audit(Path(sys.argv[1]))
    else:
        with tempfile.TemporaryDirectory(prefix="198x-drained-audit-") as tmp:
            with tarfile.open(Path(__file__).with_name("evidence.tar.gz")) as archive:
                archive.extractall(tmp, filter="data")
            result = audit(Path(tmp))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

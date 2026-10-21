#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Check raw observations and semantics; a passed audit is not qualification."""

from __future__ import annotations

import copy
import hashlib
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def check(record: dict, raw: dict[str, bytes]) -> None:
    assert record["qualified"] is False and record["capture_completed"] is True
    assert record["source_media_unchanged"] is True
    assert record["fuse_source"]["modified"] is False
    assert record["mame_executable"]["source_commit"] is None
    assert len(record["observations"]) == 10
    assert all(
        c["exit"] == 0 and c["host_timeout_seconds"] > 0 for c in record["commands"]
    )
    expected_commands = {
        "clang-version",
        "sdk",
        "fuse-build",
        "mame-version",
        "mame-1",
        "mame-2",
    } | {f"fuse-{repeat}-{srt}" for repeat in (1, 2) for srt in (0, 1, 14, 15)}
    names = [c["name"] for c in record["commands"]]
    assert len(names) == len(set(names)) and set(names) == expected_commands
    assert (
        hashlib.sha256(raw["fuse-stock"]).hexdigest()
        == record["fuse_executable"]["sha256"]
    )
    seen = set()
    count = 0
    for packet in record["observations"]:
        parsed = [json.loads(x) for x in raw[packet["raw"]].splitlines()]
        assert parsed == packet["rows"], "raw/summary mismatch"
        for row in parsed:
            key = (packet["engine"], packet["repeat"], row["srt"], row["case"])
            assert key not in seen
            seen.add(key)
            count += 1
            assert row["case"] in range(4) and row["srt"] in (0, 1, 14, 15)
            source, target = ((0, 1), (1, 3), (3, 1), (1, 1))[row["case"]]
            assert (row["from"], row["target"]) == (source, target)
            n = abs(target - source)
            assert (row["st0"], row["pcn"], row["idle_msr"]) == (32, target, 128)
            if packet["engine"] == "fuse":
                assert row["host_hz"] == 3546895
                interval = 2 * (16 - row["srt"]) * 3546895 // 1000
                assert row["completion_ticks"] == n * interval, (
                    "Fuse completion spacing"
                )
                direction = 1 if target > source else -1
                expected = [
                    {
                        "tick": i * interval,
                        "physical": source + (i + 1) * direction,
                        "pcn": source + (i + 1) * direction,
                    }
                    for i in range(n)
                ]
                assert row["steps"] == expected, "Fuse actual step spacing"
            else:
                assert packet["engine"] == "mame"
                assert row["poll_us"] == packet["poll_us"] and row["poll_us"] in (1, 32)
                samples = row["samples"]
                assert 1 <= len(samples) <= 250001
                for index, sample in enumerate(samples):
                    assert (
                        abs(sample["seconds"] - index * row["poll_us"] / 1000000) < 1e-9
                    )
                    assert (sample["st0"], sample["pcn"]) == (
                        (32, target) if index == len(samples) - 1 else (128, -1)
                    )
                ideal = n * 2 * (16 - row["srt"]) / 1000 + (n * 2.5 + 10) / 1000000
                upper = samples[-1]["seconds"]
                lower = samples[-2]["seconds"] if len(samples) > 1 else -1e-9
                assert lower < ideal + 1e-12 and ideal <= upper + 1e-12, (
                    "MAME completion bracket"
                )
                assert row["completion_seconds"] == upper
    assert count == 64
    assert seen == {
        (engine, repeat, srt, case)
        for engine in ("fuse", "mame")
        for repeat in (1, 2)
        for srt in (0, 1, 14, 15)
        for case in range(4)
    }


def main() -> None:
    archive = HERE / "runs.tar.gz"
    with tarfile.open(archive) as bundle:
        raw = {
            m.name.removeprefix("runs/"): bundle.extractfile(m).read()
            for m in bundle.getmembers()
            if m.isfile()
        }
    hashes = json.loads(raw["hashes.json"])
    for name, digest in hashes.items():
        assert hashlib.sha256(raw[name]).hexdigest() == digest, name
    record = json.loads(raw["campaign.json"])
    check(record, raw)
    source_archive = HERE / "fuse-source.tar.gz"
    assert (
        hashlib.sha256(source_archive.read_bytes()).hexdigest()
        == "d208f2df99f21275e43b7b2e0a904b80fe43d8ad63ea8f46e8e0e5ee5af128c5"
    ), "upstream source archive identity"
    with tarfile.open(source_archive) as bundle:
        upstream = {
            m.name: bundle.extractfile(m).read()
            for m in bundle.getmembers()
            if m.isfile()
        }
    manifest = json.loads(upstream["manifest.json"])
    assert manifest["source_commit"] == record["fuse_source"]["commit"]
    assert "COPYING" in manifest["files"] and len(manifest["files"]) == 17
    assert set(upstream) == {"manifest.json", "original-build-receipt.json"} | {
        "upstream/" + name for name in manifest["files"]
    }
    for name, identity in manifest["files"].items():
        data = upstream["upstream/" + name]
        assert hashlib.sha256(data).hexdigest() == identity["sha256"], name
        assert (
            hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            == identity["git_blob"]
        ), name
    for name, digest in record["fuse_source"]["files"].items():
        assert manifest["files"][name]["sha256"] == digest
    assert json.loads(upstream["original-build-receipt.json"]) == next(
        c for c in record["commands"] if c["name"] == "fuse-build"
    )
    controls = []
    for name in (
        "wrong_interval",
        "raw_summary_disagreement",
        "missing_case",
        "duplicate_case",
        "false_qualification",
        "invented_mame_source",
    ):
        bad = copy.deepcopy(record)
        fake = dict(raw)
        if name == "wrong_interval":
            packet = bad["observations"][0]
            packet["rows"][1]["steps"][1]["tick"] += 1
            fake[packet["raw"]] = "\n".join(
                json.dumps(r) for r in packet["rows"]
            ).encode()
        elif name == "raw_summary_disagreement":
            packet = bad["observations"][0]
            rows = copy.deepcopy(packet["rows"])
            rows[0]["completion_ticks"] += 1
            fake[packet["raw"]] = "\n".join(json.dumps(r) for r in rows).encode()
        elif name == "missing_case":
            packet = bad["observations"][0]
            packet["rows"].pop()
            fake[packet["raw"]] = "\n".join(
                json.dumps(r) for r in packet["rows"]
            ).encode()
        elif name == "duplicate_case":
            packet = bad["observations"][0]
            packet["rows"][1] = copy.deepcopy(packet["rows"][0])
            fake[packet["raw"]] = "\n".join(
                json.dumps(r) for r in packet["rows"]
            ).encode()
        elif name == "false_qualification":
            bad["qualified"] = True
        else:
            bad["mame_executable"]["source_commit"] = "invented"
        try:
            check(bad, fake)
        except AssertionError:
            controls.append({"control": name, "rejected": True})
        else:
            raise AssertionError("accepted " + name)
    print(
        json.dumps(
            {
                "status": "pass",
                "qualified": False,
                "operations_observed": 64,
                "fresh_processes": 10,
                "negative_controls": controls,
                "scope": "captured reference-model timing audit",
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

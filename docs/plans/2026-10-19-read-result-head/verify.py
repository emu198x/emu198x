#!/usr/bin/env python3
"""Inspect captured result-H research evidence; never a qualification verdict."""

from __future__ import annotations

import hashlib
import io
import json
import re
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def crc(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ (0x1021 if value & 0x8000 else 0)) & 0xFFFF
    return value


def archive_files(data: bytes) -> dict[str, bytes]:
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        result = {}
        for member in archive.getmembers():
            path = Path(member.name)
            if (
                not member.isfile()
                or path.is_absolute()
                or ".." in path.parts
                or member.name in result
            ):
                raise ValueError("unsafe or duplicate archive member")
            stream = archive.extractfile(member)
            assert stream is not None
            result[member.name] = stream.read()
        return result


def audit_reference(files: dict[str, bytes]) -> int:
    r = json.loads(files["campaign.json"])
    assert r["qualified"] is False and r["origin"] == "fresh_execution"
    assert r["checks_passed"] is True and r["fuse_source"]["patched"] is False
    assert r["fuse_source"]["commit"] == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    assert (
        r["fuse_source"]["files"]["peripherals/disk/upd_fdc.c"]
        == "845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d"
    )
    for name, wanted_hash in r["authored_sources"].items():
        assert sha((HERE / "reference" / name).read_bytes()) == wanted_hash
    wanted = {
        (repeat, op, n, mode)
        for repeat in [1, 2]
        for op in [0x46, 0x4C]
        for mode, n in [
            (0, 1024),
            (1, 0),
            (1, 1),
            (1, 511),
            (1, 512),
            (1, 513),
            (2, 0),
            (3, 512),
            (4, 512),
        ]
    }
    seen = set()
    pairs = {}
    for case in r["cases"]:
        repeat, op, n, mode = key = tuple(
            case[k] for k in ["repeat", "opcode", "consumed", "mode"]
        )
        assert key in wanted and key not in seen
        seen.add(key)
        o = case["observed"]
        assert (o["opcode"], o["consumed"], o["mode"]) == (op, n, mode)
        assert o["idle_msr"] == 128 and o["media_unchanged"] is True
        status = [64, 128, 0, 0, 1, 2, 2]
        if mode == 1:
            status = [64, 16, 0, 0, 1, 1 if n < 512 else 2, 2]
        elif mode in [2, 3]:
            status = [64, 5, 0, 0, 1, 1 if mode == 2 else 2, 2]
        elif mode == 4:
            status = [64, 32, 32, 0, 1, 1, 2]
        assert o["result"] == status
        pair = (op, n, mode)
        if repeat == 1:
            pairs[pair] = o
        else:
            assert pairs[pair] == o
        raw = files[case["raw_path"]]
        assert sha(raw) == case["raw_sha256"] and len(raw) == 8599
        assert raw[:3] == bytes([106, 24, 0])
        track = raw[3:6253]
        for i, (header_start, data_start) in enumerate(
            [(92, 136), (708, 752)][: 1 if mode == 3 else 2]
        ):
            header = bytes([0xA1] * 3 + [0xFE, 0, 0 if mode == 2 else 1, i + 1, 2])
            assert track[header_start : header_start + 8] == header
            assert int.from_bytes(
                track[header_start + 8 : header_start + 10], "big"
            ) == crc(header)
            data = bytes([0xA1] * 3 + [0xF8 if op == 0x4C else 0xFB] + [0x21 + i] * 512)
            assert track[data_start : data_start + 516] == data
            assert int.from_bytes(
                track[data_start + 516 : data_start + 518], "big"
            ) == crc(data) ^ (0x100 if mode == 4 and i == 0 else 0)
    assert seen == wanted and len(r["commands"]) == 38
    for command in r["commands"]:
        assert command["returncode"] == 0
        assert not files[command["name"] + ".stderr"]
    return len(seen)


def audit_runtime(files: dict[str, bytes]) -> None:
    parent = json.loads(files["198x-result-h-parent-verification.json"])
    assert parent["source_commit"] == "4f03ce6a141079869e3dce04b1cab2525db6ffb7"
    assert parent["qualified"] is False and parent["source_kind"] == "source_build"
    assert (
        parent["assembly_mastering"] == "captured_reference_replay"
        and parent["runtime"] == "fresh_execution"
    )
    for command in parent["commands"]:
        assert command["exit_code"] == 0
        assert (
            hashlib.sha256(files[command["log"]]).hexdigest() == command["log_sha256"]
        )
    tests = files["198x-result-h-runtime-tests.log"].decode()
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", tests))) == 338
    assert (
        sum(
            map(
                int,
                re.findall(
                    r"test result: ok\. \d+ passed; \d+ failed; (\d+) ignored;", tests
                ),
            )
        )
        == 63
    )
    assert "test result: FAILED." not in tests
    assert "Finished `release` profile" in files["198x-result-h-build.log"].decode()
    assert [command["exit_code"] for command in parent["regression_wrappers"]] == [
        0,
        0,
        1,
    ]
    screen = "d2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2"
    for case in ("1", "2", "wrong"):
        prefix = f"rom-{case}/"
        record = json.loads(files[prefix + "regression.json"])
        assert record["qualified"] is False
        assert record["regression_passed"] is (case != "wrong")
        assert record["fixture"]["origin"] == "captured_reference_replay"
        assert (
            record["evidence_class"] == "fresh_execution"
            and record["delivery"] == "source_build"
        )
        assert record["repositories"]["emu"]["commit"] == parent["source_commit"]
        assert record["repositories"]["emu"]["tracked_status"] == ""
        assert record["tools"]["emu"]["sha256"] == parent["binary_sha256"]
        for artifact in record["artifacts"]:
            content = files[prefix + artifact["path"]]
            assert len(content) == artifact["size"]
            assert hashlib.sha256(content).hexdigest() == artifact["sha256"]
        stages = record["stages"]
        for stage, assertions in [
            ("native_format", 519),
            ("firmware", 8),
            ("loading", 3),
        ]:
            assert (
                stages[stage]["status"] == "pass"
                and stages[stage]["assertions"] == assertions
            )
        for stage in ("asm_build", "emu_build", "assembly", "build_build", "mastering"):
            assert stages[stage]["status"] == "not_run"
        assert record["recipe"]["signature"] == (
            [90, 92] if case == "wrong" else [90, 91]
        )
        if case == "wrong":
            assert stages["execution"]["status"] == "fail"
            assert (
                stages["execution"]["reason"]
                == "signature mismatch: expected [90, 92], observed [90, 91]"
            )
            assert files[prefix + "runtime.stdout"] == files["rom-1/runtime.stdout"]
        else:
            execution = stages["execution"]
            assert execution["status"] == "pass" and execution["assertions"] == 7
            observed = execution["observed"]
            assert observed["signature_before"] == [0, 0] and observed[
                "signature_after"
            ] == [90, 91]
            assert observed["pc"] == 32822 and observed["screen_sha256"] == screen
            assert observed["writes"][-2:] == [
                {"pc": 32816, "addr": 28672, "value": 90, "size_bytes": 1},
                {"pc": 32820, "addr": 28673, "value": 91, "size_bytes": 1},
            ]
            assert execution["expected"] == {
                "signature": [90, 91],
                "pc": 32820,
                "screen_sha256": screen,
            }


def audit(files: dict[str, bytes], source: Path = HERE.parents[2]) -> dict:
    files = {**archive_files(files["checks.tar.gz"]), **files}
    r = json.loads(files["result.json"])
    assert r["qualified"] is False
    assert r["source_commit"] == "4f03ce6a141079869e3dce04b1cab2525db6ffb7"
    assert r["base_commit"] == "9d71a1b9344f47266028ff693bca2bc2fbea60a7"
    for name, wanted in r["source_sha256"].items():
        assert sha((source / name).read_bytes()) == wanted
    red_identity = json.loads(files["red-identities.json"])
    assert red_identity["exit"] == 101 and red_identity["executables"]
    red = files["red.log"].decode()
    assert "test result: FAILED. 0 passed; 3 failed;" in red
    assert (
        sha(files["red-test.rs"])
        == r["source_sha256"]["crates/nec-upd765a/tests/read_result_head.rs"]
    )
    green = files["green.log"].decode()
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", green))) == 130
    assert not re.search(r"test result: FAILED|\.\.\. FAILED", green)
    assert "test result: ok. 3 passed;" in green
    snapshot = files["snapshot.log"].decode()
    assert (
        "requested_id_head_survives_pending_read_and_result_checkpoints ... ok"
        in snapshot
    )
    assert "test result: ok. 1 passed;" in snapshot
    checks = json.loads(files["checks.json"])
    assert {c["name"] for c in checks} == {"green", "clippy", "fmt", "snapshot"}
    assert all(c["exit"] == 0 for c in checks)
    reference = audit_reference(archive_files(files["stock-fuse.tar.gz"]))
    runtime = archive_files(files["runtime-rom.tar.gz"])
    assert {name: sha(data) for name, data in runtime.items()} == r[
        "parent_validation"
    ]["files"]
    audit_runtime(runtime)
    return {
        "qualified": False,
        "controller_tests": 130,
        "focused_snapshot_tests": 1,
        "red_tests": 3,
        "reference_observations": reference,
        "runtime_plus3_tests": 338,
        "ignored_tests": 63,
        "fresh_rom_positive_runs": 2,
        "wrong_signature_rejected": True,
        "checks_passed": True,
    }


def main() -> None:
    evidence = HERE / "evidence"
    hashes = json.loads((evidence / "hashes.json").read_text())
    files = {name: (evidence / name).read_bytes() for name in hashes}
    for name, wanted in hashes.items():
        assert sha(files[name]) == wanted
    print(json.dumps(audit(files), indent=2))


if __name__ == "__main__":
    main()

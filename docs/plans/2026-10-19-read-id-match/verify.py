#!/usr/bin/env python3
"""Check retained red/green evidence without upgrading it to qualification."""

import binascii
import copy
import hashlib
import io
import json
import re
import tarfile
import tempfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
NAMES = (
    "wrong_recorded_head_is_rejected_and_matching_request_recovers",
    "wrong_recorded_size_is_rejected_and_matching_request_recovers",
    "later_exact_duplicate_is_not_hidden_by_wrong_head_or_size",
    "requested_id_head_does_not_select_the_physical_side",
)


def check(meta: dict[str, Any], logs: dict[str, str]) -> None:
    assert meta["qualified"] is False
    assert (
        meta["red"]["exit"] == 101
        and meta["red"]["passed"] == 0
        and meta["red"]["failed"] == 4
    )
    assert "test result: FAILED. 0 passed; 4 failed;" in logs["red.log"]
    assert meta["green"]["exit"] == 0 and meta["green"]["passed"] == 127
    for name in NAMES:
        assert f"test {name} ... FAILED" in logs["red.log"]
        assert f"test {name} ... ok" in logs["green.log"]
    assert (
        sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", logs["green.log"])))
        == 127
    )
    assert "test result: FAILED." not in logs["green.log"]
    assert (
        "Finished `dev` profile" in logs["clippy.log"]
        and "error:" not in logs["clippy.log"]
    )
    assert "error:" not in logs["fmt.log"] and "Diff in " not in logs["fmt.log"]


def archive_files(path: Path, expected_count: int) -> dict[str, bytes]:
    with tarfile.open(path) as archive:
        members = archive.getmembers()
        assert len(members) == expected_count
        assert len({m.name for m in members}) == expected_count
        assert all(
            m.isfile()
            and not Path(m.name).is_absolute()
            and ".." not in Path(m.name).parts
            for m in members
        )
        files = {m.name: archive.extractfile(m).read() for m in members}
    hashes = json.loads(files["hashes.json"])
    assert set(hashes) == set(files) - {"hashes.json"}
    for name, digest in hashes.items():
        assert hashlib.sha256(files[name]).hexdigest() == digest
    return files


def stock_fuse(files: dict[str, bytes]) -> None:
    provenance = json.loads(files["provenance.json"])
    assert provenance["qualified"] is False
    assert provenance["source_commit"] == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    assert (
        provenance["source_hashes"]["peripherals/disk/upd_fdc.c"]
        == "845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d"
    )
    rows = json.loads(files["runs/results.json"])
    assert len(rows) == 36
    assert len({(r["repeat"], r["opcode"], r["case"]) for r in rows}) == 36
    for row in rows:
        repeat, op, case = row["repeat"], row["opcode"], row["case"]
        assert repeat in (1, 2) and op in (0x46, 0x4C) and case in range(9)
        assert row["exit"] == 0 and row["stderr"] == ""
        assert row["media_unchanged"] is True and row["idle_msr"] == 0x80
        assert 0 < row["tstates"] < 2 * 3546900
        count = 0 if case in (1, 2) else 256 if case == 7 else 512
        assert row["received"] == count and row["other"] == 0
        assert row["counts_a1_a2"] == ([0, count] if case in (3, 4, 5) else [count, 0])
        status = [
            0x40,
            5 if case in (1, 2) else 0x20 if case == 8 else 0x80,
            0x20 if case == 8 else 0,
        ]
        assert row["result"] == status + [
            0,
            1 if case == 6 else 0,
            1,
            1 if case == 7 else 2,
        ]
        stem = f"{repeat}-{op:02x}-{case}"
        assert files[f"runs/{stem}.stderr"] == b""
        output = json.loads(files[f"runs/{stem}.stdout"])
        for key, value in output.items():
            assert row[key] == value
        raw = files[f"runs/{stem}.raw"]
        assert len(raw) == 3 + 6250 + 3 * 782 and raw[:3] == bytes([0x6A, 0x18, 0])
        track = raw[3:6253]
        ids = []
        pos = 0
        while True:
            pos = track.find(b"\xa1\xa1\xa1\xfe", pos)
            if pos < 0:
                break
            header = track[pos : pos + 8]
            assert binascii.crc_hqx(header, 0xFFFF) == int.from_bytes(
                track[pos + 8 : pos + 10], "big"
            )
            c, h, r, n = header[4:]
            ids.append((c, h, r, n))
            mark = b"\xf8" if op == 0x4C else b"\xfb"
            data_pos = track.index(b"\xa1\xa1\xa1" + mark, pos + 10)
            size = 128 << n
            data = track[data_pos : data_pos + 4 + size]
            recorded_crc = int.from_bytes(
                track[data_pos + 4 + size : data_pos + 6 + size], "big"
            )
            assert (binascii.crc_hqx(data, 0xFFFF) == recorded_crc) is (case != 8)
            assert data[4:] == bytes([0xA0 + len(ids)]) * size
            pos += 10
        first_h = 1 if case in (1, 3, 5, 6) else 0
        first_n = 1 if case in (2, 4, 5, 7) else 2
        assert ids == [(0, first_h, 1, first_n)] + (
            [(0, 0, 1, 2)] if case in (3, 4, 5) else []
        )
    for op in (0x46, 0x4C):
        for case in range(9):
            pair = [r for r in rows if r["opcode"] == op and r["case"] == case]
            assert {
                k: v for k, v in pair[0].items() if k not in ("repeat", "command")
            } == {k: v for k, v in pair[1].items() if k not in ("repeat", "command")}


def runtime_rom(files: dict[str, bytes]) -> None:
    parent = json.loads(files["198x-service-parent-verification.json"])
    assert parent["source_commit"] == "8db4b82a38844bcbd45df28cfd3626190910ebee"
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
    tests = files["198x-service-runtime-tests.log"].decode()
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", tests))) == 337
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
    assert "Finished `release` profile" in files["198x-service-build.log"].decode()
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


def check_archives(meta: dict[str, Any]) -> list[str]:
    reference = archive_files(
        ROOT / "evidence/stock-fuse.tar.gz", meta["independent_reference"]["files"]
    )
    runtime = archive_files(
        ROOT / "evidence/runtime-rom.tar.gz", meta["parent_validation"]["files"]
    )
    stock_fuse(reference)
    runtime_rom(runtime)
    controls = []
    broken = dict(reference)
    rows = json.loads(broken["runs/results.json"])
    rows[1]["received"] = 512
    broken["runs/results.json"] = json.dumps(rows).encode()
    cases = [("wrong_reference_payload", lambda: stock_fuse(broken))]
    missing = dict(runtime)
    missing.pop("rom-1/runtime.stdout")
    cases.append(("missing_runtime_artifact", lambda: runtime_rom(missing)))
    zero = dict(runtime)
    record = json.loads(zero["rom-1/regression.json"])
    record["stages"]["native_format"]["assertions"] = 0
    zero["rom-1/regression.json"] = json.dumps(record).encode()
    cases.append(("zero_assertions", lambda: runtime_rom(zero)))
    wrong = dict(runtime)
    record = json.loads(wrong["rom-wrong/regression.json"])
    record["stages"]["execution"]["status"] = "pass"
    wrong["rom-wrong/regression.json"] = json.dumps(record).encode()
    cases.append(("false_execution_pass", lambda: runtime_rom(wrong)))
    for name, callback in cases:
        try:
            callback()
        except (AssertionError, KeyError):
            controls.append(name)
        else:
            raise AssertionError(f"archive mutation accepted: {name}")
    with tempfile.TemporaryDirectory(prefix="198x-id-evidence-control-") as directory:
        corrupt = Path(directory) / "corrupt.tar.gz"
        with tarfile.open(corrupt, "w:gz") as archive:
            for name, content in reference.items():
                if name == "runs/1-46-0.raw":
                    content = content[:-1] + bytes([content[-1] ^ 1])
                info = tarfile.TarInfo(name)
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))
        try:
            archive_files(corrupt, len(reference))
        except AssertionError:
            controls.append("corrupt_reference_artifact")
        else:
            raise AssertionError("corrupt evidence passed its hash check")
    return controls


def main() -> None:
    e = ROOT / "evidence"
    for name, digest in json.loads((e / "hashes.json").read_text()).items():
        assert hashlib.sha256((e / name).read_bytes()).hexdigest() == digest
    meta = json.loads((e / "result.json").read_text())
    logs = {
        name: (e / name).read_text()
        for name in ["red.log", "green.log", "clippy.log", "fmt.log"]
    }
    check(meta, logs)
    controls = check_archives(meta)
    mutations = [
        (
            "missing_test",
            lambda m, l: l.update(
                {"green.log": l["green.log"].replace(f"test {NAMES[0]} ... ok", "")}
            ),
        ),
        ("empty_tests", lambda m, l: l.update({"green.log": ""})),
        ("wrong_exit", lambda m, l: m["red"].update({"exit": 0})),
    ]
    for name, mutate in mutations:
        changed_meta, changed_logs = copy.deepcopy(meta), dict(logs)
        mutate(changed_meta, changed_logs)
        try:
            check(changed_meta, changed_logs)
        except AssertionError:
            controls.append(name)
        else:
            raise AssertionError(f"negative control accepted: {name}")
    print(
        json.dumps(
            {
                "qualified": False,
                "evidence_checks": "pass",
                "controller_tests": 127,
                "rejected_controls": controls,
            }
        )
    )


if __name__ == "__main__":
    main()

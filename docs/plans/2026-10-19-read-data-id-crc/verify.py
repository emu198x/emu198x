#!/usr/bin/env python3
"""Audit captured #1852 evidence; this does not execute an emulator."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = "e72ca90260bf788720ad8fe88b51d0e9b6a665f4"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unpack(path: Path) -> dict[str, bytes]:
    with tarfile.open(path) as archive:
        result = {}
        for member in archive.getmembers():
            p = Path(member.name)
            assert member.isfile() and not p.is_absolute() and ".." not in p.parts
            assert member.name not in result
            stream = archive.extractfile(member)
            assert stream is not None
            result[member.name] = stream.read()
        assert result
        return result


def audit_runtime(files: dict[str, bytes]) -> None:
    checks = json.loads(files["checks.json"])
    assert [c["exit"] for c in checks] == [0, 0, 1]
    assert [c["name"] for c in checks] == [
        "positive-1",
        "positive-2",
        "wrong-signature",
    ]
    for name in ["positive-1", "positive-2", "wrong-signature"]:
        r = json.loads(files[name + "/regression.json"])
        positive = name != "wrong-signature"
        assert r["qualified"] is False and r["regression_passed"] is positive
        assert r["fixture"]["origin"] == "captured_reference_replay"
        assert (
            r["evidence_class"] == "fresh_execution" and r["delivery"] == "source_build"
        )
        assert r["repositories"]["emu"]["commit"] == SOURCE
        assert r["repositories"]["emu"]["tracked_status"] == ""
        assert (
            r["tools"]["emu"]["sha256"]
            == "21bab44e81fad81a2d402e32bb5ed7867bc6ebbfe050b6ce540aa88f35e1cb82"
        )
        assert (
            r["configuration"]["id"]
            == "spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1"
        )
        artifact_names = {artifact["path"] for artifact in r["artifacts"]}
        assert len(artifact_names) == len(r["artifacts"])
        assert {
            "runtime.stdout",
            "runtime.stderr",
            "session.json",
            "diagnostic.bin",
            "diagnostic.dsk",
            "screen.png",
        } <= artifact_names
        for artifact in r["artifacts"]:
            data = files[name + "/" + artifact["path"]]
            assert len(data) == artifact["size"] and sha(data) == artifact["sha256"]
        for stage, count in [("native_format", 519), ("firmware", 8), ("loading", 3)]:
            assert r["stages"][stage]["status"] == "pass"
            assert r["stages"][stage]["assertions"] == count
        assert all(
            r["stages"][s]["status"] == "not_run" for s in ["assembly", "mastering"]
        )
        execution = r["stages"]["execution"]
        if positive:
            assert execution["status"] == "pass" and execution["assertions"] == 7
            observed = execution["observed"]
            assert observed["signature_before"] == [0, 0]
            assert observed["signature_after"] == [90, 91]
            assert (
                observed["screen_sha256"]
                == "d2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2"
            )
            assert observed["writes"][-2:] == [
                {"pc": 32816, "addr": 28672, "value": 90, "size_bytes": 1},
                {"pc": 32820, "addr": 28673, "value": 91, "size_bytes": 1},
            ]
        else:
            assert execution["status"] == "fail"
            assert (
                execution["reason"]
                == "signature mismatch: expected [90, 92], observed [90, 91]"
            )
        runtime = [c for c in r["commands"] if c["name"] == "runtime"]
        assert len(runtime) == 1 and runtime[0]["returncode"] == 0


def audit(evidence: Path = HERE / "evidence") -> dict:
    hashes = json.loads((evidence / "hashes.json").read_text())
    assert (
        hashes
        and {
            "triage.tar.gz",
            "runtime-rom.tar.gz",
            "current-source.json",
            "checks.tar.gz",
            "build.log",
            "clippy.log",
            "controller-red.log",
            "native-red.log",
        }
        <= hashes.keys()
    )
    for name, digest in hashes.items():
        assert sha((evidence / name).read_bytes()) == digest
    source = json.loads((evidence / "current-source.json").read_text())
    assert source["qualified"] is False and source["source_commit"] == SOURCE
    assert set(source["executed_source_sha256"]) == {
        "crates/nec-upd765a/src/lib.rs",
        "crates/nec-upd765a/tests/read_progress.rs",
        "crates/runtime-sinclair-zx-spectrum/src/snapshot.rs",
        "crates/runtime-sinclair-zx-spectrum/src/variants.rs",
    }
    summary = json.loads((evidence / "result.json").read_text())
    assert (
        summary["schema_version"] == 1
        and summary["qualified"] is False
        and summary["source_commit"] == SOURCE
    )
    assert summary["stages"]["controller_runtime_machine_tests"] == {
        "status": "pass",
        "passed": 480,
        "ignored": 63,
    }
    assert summary["stages"]["fresh_assembly_mastering"]["status"] == "not_run"
    assert (
        summary["legacy_v6"]["fixtures"] == 8
        and summary["legacy_v6"]["schema_changed"] is False
    )
    for name, digest in source["executed_source_sha256"].items():
        assert sha((HERE.parents[2] / name).read_bytes()) == digest
    assert source["runtime"]["delivery"] == "source_build"
    assert (
        "test result: FAILED. 1 passed; 2 failed;"
        in (evidence / "controller-red.log").read_text()
    )
    assert (
        "test result: FAILED. 0 passed; 2 failed;"
        in (evidence / "native-red.log").read_text()
    )
    tests = unpack(evidence / "checks.tar.gz")["final-tests.log"].decode()
    assert not re.search(r"test result: FAILED|\.\.\. FAILED", tests)
    assert sum(map(int, re.findall(r"test result: ok\. (\d+) passed;", tests))) == 480
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
    for name in [
        "data_reads_reject_id_crc_before_payload_and_recover_without_changing_media",
        "data_reads_transfer_clean_and_data_crc_fields_before_result",
        "data_reads_do_not_transfer_later_bad_id_or_publish_its_data_mark",
        "old_v6_bad_id_reads_restore_to_result_before_any_further_payload",
        "bad_id_results_and_valid_data_reads_keep_native_roundtrips",
        "legacy_bad_id_normalization_discards_a_previously_visited_data_mark",
    ]:
        assert name + " ... ok" in tests
    assert "Finished `release` profile" in (evidence / "build.log").read_text()
    assert "Finished `dev` profile" in (evidence / "clippy.log").read_text()
    audit_runtime(unpack(evidence / "runtime-rom.tar.gz"))
    triage = unpack(evidence / "triage.tar.gz")
    assert (
        triage["capture_legacy.rs"].rstrip()
        == (HERE / "reference/capture_legacy.rs").read_bytes().rstrip()
    )
    record = json.loads(triage["runs/campaign.json"])
    for name, digest in record["authored_sources"].items():
        assert sha((HERE / "reference" / name).read_bytes()) == digest
    spec = importlib.util.spec_from_file_location(
        "id_crc_reference_verify", HERE / "reference/verify.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    reference = module.audit()
    assert reference["fuse_observations"] == 12 and reference["emu_observations"] == 12
    # These are baseline bug observations, separate from the green runtime.
    baseline = json.loads(triage["emu.json"])
    assert baseline["source_commit"] == "5cded0a6b646f493d7cc2abc8292eb26c3837f49"
    for name in [
        "46-0-0",
        "46-0-1",
        "46-179-0",
        "46-179-1",
        "4c-0-0",
        "4c-0-1",
        "4c-179-0",
        "4c-179-1",
    ]:
        captured = triage["legacy/" + name + ".postcard"]
        assert (
            captured
            == (
                HERE.parents[2]
                / "crates/runtime-sinclair-zx-spectrum/tests/fixtures/read-data-id-crc"
                / (name + ".postcard")
            ).read_bytes()
        )
    return {
        "qualified": False,
        "checks_passed": True,
        "tests_passed": 480,
        "tests_ignored": 63,
        "fresh_rom_runs": 2,
        "wrong_signature_rejected": True,
        "baseline_fuse_observations": 12,
        "baseline_emu_observations": 12,
        "legacy_v6_controller_fixtures": 8,
    }


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2))

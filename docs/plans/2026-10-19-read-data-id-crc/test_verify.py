"""Controls that make the retained runtime auditor fail."""

from __future__ import annotations

import copy
import json
import unittest

import verify


class RuntimeAuditControls(unittest.TestCase):
    def setUp(self) -> None:
        self.files = verify.unpack(verify.HERE / "evidence/runtime-rom.tar.gz")

    def reject_record(self, mutate) -> None:
        record = json.loads(self.files["positive-1/regression.json"])
        mutate(record)
        self.files["positive-1/regression.json"] = json.dumps(record).encode()
        with self.assertRaises(AssertionError):
            verify.audit_runtime(self.files)

    def test_complete_packet(self) -> None:
        self.assertTrue(verify.audit()["checks_passed"])

    def test_no_artifacts(self) -> None:
        self.reject_record(lambda r: r.update(artifacts=[]))

    def test_missing_runtime_log(self) -> None:
        self.files.pop("positive-1/runtime.stdout")
        with self.assertRaises(KeyError):
            verify.audit_runtime(self.files)

    def test_duplicate_artifact(self) -> None:
        self.reject_record(
            lambda r: r["artifacts"].append(copy.deepcopy(r["artifacts"][0]))
        )

    def test_wrong_source(self) -> None:
        self.reject_record(lambda r: r["repositories"]["emu"].update(commit="0" * 40))

    def test_zero_execution_assertions(self) -> None:
        self.reject_record(lambda r: r["stages"]["execution"].update(assertions=0))

    def test_loading_skipped(self) -> None:
        self.reject_record(lambda r: r["stages"]["loading"].update(status="not_run"))

    def test_wrong_transition(self) -> None:
        self.reject_record(
            lambda r: r["stages"]["execution"]["observed"].update(
                signature_before=[90, 91]
            )
        )

    def test_false_qualification(self) -> None:
        self.reject_record(lambda r: r.update(qualified=True))

    def test_wrong_control_exit(self) -> None:
        checks = json.loads(self.files["checks.json"])
        checks[-1]["exit"] = 0
        self.files["checks.json"] = json.dumps(checks).encode()
        with self.assertRaises(AssertionError):
            verify.audit_runtime(self.files)

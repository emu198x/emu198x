"""Prove the captured runtime auditor rejects false claims."""

from __future__ import annotations

import copy
import json
import unittest

import verify


class EvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = verify.unpack(verify.HERE / "evidence/runtime-rom.tar.gz")

    def reject(self, change):
        files = copy.deepcopy(self.files)
        r = json.loads(files["positive-1/regression.json"])
        change(r)
        files["positive-1/regression.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            verify.audit_runtime(files)

    def test_complete(self):
        self.assertTrue(verify.audit()["checks_passed"])

    def test_zero_loading_checks(self):
        self.reject(lambda r: r["stages"]["loading"].update(assertions=0))

    def test_loaded_payload_is_not_execution(self):
        self.reject(lambda r: r["stages"]["execution"]["observed"].update(writes=[]))

    def test_captured_run_cannot_be_full_qualification(self):
        self.reject(lambda r: r.update(qualified=True))

    def test_wrong_runtime_identity(self):
        self.reject(lambda r: r["repositories"]["emu"].update(commit="0" * 40))

    def test_payload_corruption(self):
        files = copy.deepcopy(self.files)
        files["positive-1/diagnostic.bin"] = b"corrupt"
        with self.assertRaises(AssertionError):
            verify.audit_runtime(files)

    def test_empty_artifact_list(self):
        self.reject(lambda r: r.update(artifacts=[]))

    def test_missing_raw_runtime_log(self):
        files = copy.deepcopy(self.files)
        del files["positive-1/runtime.stdout"]
        with self.assertRaises(KeyError):
            verify.audit_runtime(files)

    def test_wrong_expected_must_fail(self):
        files = copy.deepcopy(self.files)
        r = json.loads(files["wrong-signature/regression.json"])
        r["stages"]["execution"]["status"] = "pass"
        files["wrong-signature/regression.json"] = json.dumps(r).encode()
        with self.assertRaises(AssertionError):
            verify.audit_runtime(files)


if __name__ == "__main__":
    unittest.main()

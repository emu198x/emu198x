import json
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import verify


class AuditControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = verify.HERE
        cls.temporary = tempfile.TemporaryDirectory(prefix="198x-read-id-tests-")
        root = Path(cls.temporary.name)
        with tarfile.open(verify.HERE.parent / "evidence/triage.tar.gz") as archive:
            for member in archive:
                name = Path(member.name)
                assert (
                    member.isfile()
                    and not name.is_absolute()
                    and ".." not in name.parts
                )
                stream = archive.extractfile(member)
                assert stream is not None
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(stream.read())
        verify.HERE = root

    @classmethod
    def tearDownClass(cls):
        verify.HERE = cls.original
        cls.temporary.cleanup()

    def test_capture(self):
        self.assertTrue(verify.audit()["checks_passed"])

    def reject(self, mutate):
        original = verify.Path.read_text
        record = json.loads((verify.HERE / "runs/campaign.json").read_text())
        mutate(record)

        def read(path, *args, **kwargs):
            if path == verify.HERE / "runs/campaign.json":
                return json.dumps(record)
            return original(path, *args, **kwargs)

        with (
            patch.object(verify.Path, "read_text", read),
            self.assertRaises(AssertionError),
        ):
            verify.audit()

    def test_invented_payload(self):
        self.reject(lambda r: r["cases"][1]["observed"].update(consumed=512))

    def test_missing_cases(self):
        self.reject(lambda r: r.update(cases=[]))

    def test_qualified(self):
        self.reject(lambda r: r.update(qualified=True))

    def test_patched_reference(self):
        self.reject(lambda r: r["fuse_source"].update(patched=True))

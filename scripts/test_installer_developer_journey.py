"""Verify that installer evidence binds both archive and installed bytes."""

import hashlib
import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('installer-developer-journey.py'))
assert SPEC is not None and SPEC.loader is not None
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


class InstallerIdentityControls(unittest.TestCase):
    def test_real_container_and_extracted_payload_keep_distinct_identities(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive, binary = root / 'tool.zip', root / 'tool.exe'
            with zipfile.ZipFile(archive, 'w') as zipped:
                zipped.writestr('tool.exe', b'published executable payload')
            with zipfile.ZipFile(archive) as zipped:
                binary.write_bytes(zipped.read('tool.exe'))
            archive_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
            record = installer.archive_install_record({'sha256': archive_hash, 'release': 'selected-public-release'}, binary)
            self.assertEqual(record['archive_sha256'], archive_hash)
            self.assertEqual(record['sha256'], hashlib.sha256(binary.read_bytes()).hexdigest())
            self.assertNotEqual(record['sha256'], record['archive_sha256'])
            self.assertEqual(record['route'], 'advertised ZIP, not an installer')

    def test_missing_installed_payload_cannot_produce_an_identity_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory, self.assertRaises(FileNotFoundError):
            installer.archive_install_record({'sha256': '0' * 64}, Path(directory) / 'missing.exe')


if __name__ == '__main__':
    unittest.main()

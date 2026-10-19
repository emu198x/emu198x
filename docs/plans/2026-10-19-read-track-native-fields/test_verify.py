# SPDX-License-Identifier: GPL-2.0-or-later
"""Adversarial native evidence controls; updates hashes to test deeper guards."""

from __future__ import annotations

import json
import tarfile
import tempfile
import unittest
from pathlib import Path

import verify

HERE = Path(__file__).resolve().parent


class NativeEvidenceControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="198x-native-audit-")
        cls.root = Path(cls.temp.name)
        with tarfile.open(HERE / "evidence.tar.gz") as a:
            a.extractall(cls.root, filter="data")
        cls.original = {
            str(p.relative_to(cls.root)): p.read_bytes()
            for p in cls.root.rglob("*")
            if p.is_file()
        }
        cls.changed = set()

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def setUp(self) -> None:
        for name in self.changed:
            (self.root / name).write_bytes(self.original[name])
        self.changed.clear()

    def write(self, name: str, data: bytes) -> None:
        (self.root / name).write_bytes(data)
        self.changed.add(name)

    def rehash(self, name: str) -> None:
        p = self.root / "hashes.json"
        d = json.loads(p.read_text())
        d[name] = verify.sha((self.root / name).read_bytes())
        self.write("hashes.json", json.dumps(d).encode())

    def change(self, callback, sync: bool = False) -> None:
        d = json.loads((self.root / "campaign.json").read_text())
        callback(d)
        self.write("campaign.json", json.dumps(d).encode())
        self.rehash("campaign.json")
        if sync:
            for execution in d["executions"]:
                name = f"positive-{execution['repeat']}.stdout"
                text = (self.root / name).read_text()
                iterator = iter(execution["observations"])
                lines = []
                for line in text.splitlines():
                    if "READ_TRACK_NATIVE " in line:
                        line = (
                            line.split("READ_TRACK_NATIVE ", 1)[0]
                            + "READ_TRACK_NATIVE "
                            + json.dumps(next(iterator))
                        )
                    lines.append(line)
                self.write(name, ("\n".join(lines) + "\n").encode())
                self.rehash(name)

    def bad(self) -> None:
        with self.assertRaises(ValueError):
            verify.audit(self.root)

    def test_baseline(self) -> None:
        self.assertEqual(verify.audit(self.root)["fresh_native_restores"], 360)

    def test_missing_checkpoint(self) -> None:
        self.change(lambda d: d["executions"][0]["observations"].pop())
        self.bad()

    def test_duplicate_checkpoint(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"].append(
                d["executions"][0]["observations"][0]
            )
        )
        self.bad()

    def test_zero_checkpoints(self) -> None:
        self.change(lambda d: d["executions"][0].update(observations=[]))
        self.bad()

    def test_missing_repeat(self) -> None:
        self.change(lambda d: d["executions"].pop())
        self.bad()

    def test_fake_replay(self) -> None:
        self.change(lambda d: d.update(evidence_class="captured_reference_replay"))
        self.bad()

    def test_wrong_expected_payload(self) -> None:
        def mutate(d):
            d["executions"][0]["observations"][0]["expected_data"][0] ^= 1

        self.change(mutate, sync=True)
        with self.assertRaisesRegex(ValueError, "wrong expected continuation payload"):
            verify.audit(self.root)

    def test_wrong_expected_result(self) -> None:
        def mutate(d):
            d["executions"][0]["observations"][0]["expected_result"][0] ^= 64

        self.change(mutate, sync=True)
        with self.assertRaisesRegex(ValueError, "wrong expected result"):
            verify.audit(self.root)

    def test_wrong_size(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0].update(payload_total=127),
            sync=True,
        )
        self.bad()

    def test_truncated_payload(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0]["observed_data"].pop(),
            sync=True,
        )
        self.bad()

    def test_truncated_result(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0]["observed_result"].pop(),
            sync=True,
        )
        self.bad()

    def test_lost_recovery(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0].update(recovery_msr=208),
            sync=True,
        )
        self.bad()

    def test_source_replayed(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0].update(
                live_unchanged=False
            ),
            sync=True,
        )
        self.bad()

    def test_false_final_state(self) -> None:
        self.change(
            lambda d: d["executions"][0]["observations"][0].update(
                final_state_equal=False
            ),
            sync=True,
        )
        self.bad()

    def test_corrupt_checkpoint_rehashed(self) -> None:
        name = next(
            n
            for n in self.original
            if n.startswith("repeat-1/") and n.endswith(".state")
        )
        data = bytearray((self.root / name).read_bytes())
        data[0] = 5
        self.write(name, data)
        self.rehash(name)
        with self.assertRaisesRegex(ValueError, "native v6 checkpoint"):
            verify.audit(self.root)

    def test_truncated_checkpoint_rehashed(self) -> None:
        name = next(
            n
            for n in self.original
            if n.startswith("repeat-1/") and n.endswith(".state")
        )
        self.write(name, self.original[name][:16])
        self.rehash(name)
        self.bad()

    def test_negative_must_reject(self) -> None:
        def mutate(d):
            next(c for c in d["commands"] if c["name"] == "wrong-expected").update(
                returncode=0
            )

        self.change(mutate)
        self.bad()

    def test_negative_wrong_reason(self) -> None:
        self.write("wrong-expected.stdout", b"failed before assertions\n")
        self.write("wrong-expected.stderr", b"")
        self.rehash("wrong-expected.stdout")
        self.rehash("wrong-expected.stderr")
        self.bad()


if __name__ == "__main__":
    unittest.main()

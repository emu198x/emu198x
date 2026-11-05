"""Exercise package qualification with Homebrew's explicit formula trust."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
import urllib.request
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("test-linux-packages.py")
SPEC = importlib.util.spec_from_file_location("linux_packages", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
packages = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(packages)


class LinuxPackagesTest(unittest.TestCase):
    def test_trust_precedes_checksum_rejection_and_real_installation(self) -> None:
        with tempfile.TemporaryDirectory(prefix="linux-package-trust-") as temporary:
            root = Path(temporary)
            dist = root / "target/distrib"
            dist.mkdir(parents=True)
            tap = root / "tap"
            (tap / "Formula").mkdir(parents=True)
            archive = "emu198x-spectrum-x86_64-unknown-linux-gnu.tar.gz"
            original = b"release package fixture"
            expected = hashlib.sha256(original).hexdigest()
            (dist / archive).write_bytes(original)
            (dist / "emu198x-spectrum.rb").write_text(
                f'url "https://example.invalid/{archive}"\nsha256 "{expected}"\n'
            )
            manifest = root / "plan.json"
            manifest.write_text(
                json.dumps({"releases": [{"artifacts": ["emu198x-spectrum.rb"]}]})
            )
            commands: list[tuple[str, ...]] = []
            trusted: set[str] = set()
            fetches: list[bool] = []

            def brew(*args: str, **kwargs: object) -> subprocess.CompletedProcess[str]:
                commands.append(args)
                output, status = "", 0
                if args[:2] == ("brew", "--repository"):
                    output = str(tap)
                elif args[:2] == ("brew", "trust"):
                    self.assertEqual(
                        args[2],
                        "--formula",
                        "Only this generated formula is authorised",
                    )
                    trusted.add(args[3])
                elif args[:2] in [("brew", "fetch"), ("brew", "install")]:
                    formula = args[-1]
                    if formula not in trusted:
                        output, status = (
                            "Refusing to load formula from untrusted tap",
                            1,
                        )
                    elif args[1] == "fetch":
                        text = (tap / "Formula/emu198x-spectrum.rb").read_text()
                        url = text.split('url "')[1].split('"')[0]
                        with urllib.request.urlopen(url, timeout=5) as response:
                            actual = hashlib.sha256(response.read()).hexdigest()
                        fetches.append(actual == expected)
                        if actual != expected:
                            output, status = (
                                f"SHA256 checksum mismatch: actual {actual}",
                                1,
                            )
                if status and kwargs.get("check", True):
                    raise subprocess.CalledProcessError(status, args, output)
                return subprocess.CompletedProcess(
                    args, status, stdout=output, stderr=""
                )

            # `run` is the checked command path; subprocess.run is the
            # deliberately failing checksum probe. Enforce the same trust
            # and bytes contract for both, rather than accepting every stub.
            with (
                patch.dict(os.environ, {"GITHUB_ACTIONS": "true"}),
                patch.object(
                    packages.sys,
                    "argv",
                    [str(SCRIPT), str(manifest), "x86_64-unknown-linux-gnu"],
                ),
                patch.object(
                    packages.os, "uname", return_value=SimpleNamespace(machine="x86_64")
                ),
                patch.object(packages, "run", side_effect=brew),
                patch.object(
                    packages.subprocess,
                    "run",
                    side_effect=lambda args, **kwargs: brew(*args, **kwargs),
                ),
            ):
                previous = Path.cwd()
                try:
                    os.chdir(root)
                    packages.main()
                finally:
                    os.chdir(previous)
            self.assertEqual(
                fetches,
                [False, True],
                "Corrupt bytes must fail before the genuine archive passes",
            )
            self.assertIn(
                ("brew", "install", "198x/package-test/emu198x-spectrum"), commands
            )
            self.assertTrue(
                any(
                    command[:2] == ("bash", "scripts/smoke-package.sh")
                    for command in commands
                )
            )
            self.assertEqual(
                commands[-1],
                ("brew", "uninstall", "198x/package-test/emu198x-spectrum"),
            )


if __name__ == "__main__":
    unittest.main()

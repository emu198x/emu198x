#!/usr/bin/env python3
"""Rejection controls for producer locks, artifact receipts and publication gates."""

from __future__ import annotations

import copy
import io
import json
import re
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import tomllib
from source_lock import (
    TARGETS,
    archive_records,
    build,
    guarded_command,
    native_target,
    package_set,
    receipt_name,
    sha,
    source_identity,
    suite_packages,
    verify,
    verify_receipt,
)

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ("emu198x-one", "emu198x-two")
EXPECTED = dict.fromkeys(PACKAGES, "1.0.0")


def plan_for(target: str, packages: tuple[str, ...] = PACKAGES) -> dict[str, Any]:
    extension = "zip" if target.endswith("msvc") else "tar.gz"
    archives = {name: f"{name}-{target}.{extension}" for name in packages}
    return {
        "releases": [
            {"app_name": name, "app_version": "1.0.0", "artifacts": [archive]}
            for name, archive in archives.items()
        ],
        "artifacts": {
            archive: {"target_triples": [target], "checksum": archive + ".sha256"}
            for archive in archives.values()
        },
    }


def write_archives(dist: Path, target: str, plan: dict[str, Any]) -> None:
    dist.mkdir(exist_ok=True)
    if target.endswith("msvc"):
        binary = bytearray(96)
        binary[:2] = b"MZ"
        struct.pack_into("<I", binary, 0x3C, 64)
        binary[64:68] = b"PE\0\0"
        struct.pack_into("<H", binary, 68, 0x8664)
    elif target.endswith("darwin"):
        binary = bytearray.fromhex("cffaedfe") + bytearray(28)
        struct.pack_into(
            "<I", binary, 4, 0x100000C if target.startswith("aarch64") else 0x1000007
        )
    else:
        binary = bytearray.fromhex("7f454c460201") + bytearray(58)
        struct.pack_into("<H", binary, 18, 183 if target.startswith("aarch64") else 62)
    for release in plan["releases"]:
        name = release["app_name"]
        path = dist / release["artifacts"][0]
        if target.endswith("msvc"):
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr(name + ".exe", binary)
        else:
            with tarfile.open(path, "w:gz") as archive:
                member = tarfile.TarInfo(f"{name}-{target}/{name}")
                member.size = len(binary)
                archive.addfile(member, io.BytesIO(binary))
        path.with_name(path.name + ".sha256").write_text(
            f"{sha(path.read_bytes())}  {path.name}\n"
        )


class ProducerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(prefix="emu198x-lock-control-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.git("init", "--quiet")
        (self.root / "Cargo.lock").write_bytes(
            b'version = 4\n[[package]]\nname="fixture"\nversion="1.0.0"\n'
        )
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers=["one","two"]\n[workspace.package]\nversion="1.0.0"\n'
        )
        for directory, name in zip(("one", "two"), PACKAGES):
            crate = self.root / directory
            (crate / "src").mkdir(parents=True)
            (crate / "src/main.rs").write_text("fn main() {}")
            (crate / "Cargo.toml").write_text(
                f'[package]\nname="{name}"\nversion.workspace=true\n'
                "[package.metadata.dist]\ndist=true\n"
            )
        self.git("add", ".")
        # A disposable fixture has no signer; ordinary commit hooks still run.
        self.git(
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "Fixture lock",
        )

    def git(self, *args: str) -> bytes:
        return subprocess.run(
            ["git", "-c", "core.fsmonitor=false", *args],
            cwd=self.root,
            capture_output=True,
            check=True,
            timeout=10,
        ).stdout

    def command(self, code: str) -> list[str]:
        return [sys.executable, "-c", code]

    def test_successful_command_preserves_committed_bytes(self) -> None:
        expected = source_identity(self.root)
        manifest = self.root / "manifest.json"
        actual = guarded_command(self.root, self.command("print('{}')"), manifest)
        self.assertEqual(actual, expected)
        self.assertEqual(json.loads(manifest.read_text()), {})
        self.assertEqual(
            actual["cargo_lock_sha256"], sha(self.git("show", "HEAD:Cargo.lock"))
        )

    def test_bad_lock_rejects_before_packaging(self) -> None:
        lock = self.root / "Cargo.lock"
        original = lock.read_bytes()
        for mutation in (b"wrong", b"", None):
            lock.write_bytes(original)
            if mutation is None:
                lock.unlink()
            else:
                lock.write_bytes(mutation)
            with (
                self.subTest(mutation=mutation),
                self.assertRaises((ValueError, OSError)),
            ):
                guarded_command(
                    self.root,
                    self.command("open('executed','w').write('yes')"),
                    self.root / "manifest.json",
                )
            self.assertFalse((self.root / "executed").exists())

    def test_bad_lock_during_successful_command_rejects(self) -> None:
        lock = self.root / "Cargo.lock"
        original = lock.read_bytes()
        for action in ("p.write_text('wrong')", "p.write_text('')", "p.unlink()"):
            lock.write_bytes(original)
            with self.subTest(action=action), self.assertRaises((ValueError, OSError)):
                guarded_command(
                    self.root,
                    self.command(
                        "from pathlib import Path;p=Path('Cargo.lock');"
                        + action
                        + ";print('{}')"
                    ),
                    self.root / "manifest.json",
                )

    def test_failed_command_rejects(self) -> None:
        with self.assertRaises(subprocess.CalledProcessError):
            guarded_command(
                self.root,
                self.command("raise SystemExit(9)"),
                self.root / "manifest.json",
            )

    def test_missing_committed_lock_rejects(self) -> None:
        self.git("rm", "Cargo.lock")
        self.git(
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "Remove fixture lock",
        )
        (self.root / "Cargo.lock").write_text("an untracked replacement")
        with self.assertRaises(subprocess.CalledProcessError):
            source_identity(self.root)

    def test_real_native_archive_receipts_cover_both_packages(self) -> None:
        target = native_target()
        plan = plan_for(target)
        dist = self.root / "distrib"
        write_archives(dist, target, plan)
        (self.root / "input.json").write_text(json.dumps(plan))
        manifest = self.root / "manifest.json"
        argv = self.command(
            "from pathlib import Path;print(Path('input.json').read_text())"
        )
        receipts = build(self.root, target, manifest, dist, argv)
        self.assertEqual(len(receipts), 2)
        report = verify(self.root, target, manifest, dist)
        self.assertTrue(report["verified"])
        self.assertEqual({r["package"] for r in report["receipts"]}, set(PACKAGES))
        with self.assertRaisesRegex(ValueError, "stale"):
            build(self.root, target, manifest, dist, argv)
        last = dist / receipt_name(PACKAGES[-1], target)
        original = last.read_text()
        last.unlink()
        with self.assertRaises(FileNotFoundError):
            verify(self.root, target, manifest, dist)
        changed = json.loads(original)
        changed["archive_sha256"] = "0" * 64
        last.write_text(json.dumps(changed))
        with self.assertRaisesRegex(ValueError, "archive_sha256 differs"):
            verify(self.root, target, manifest, dist)

    def test_every_receipt_identity_must_match_observations(self) -> None:
        source = source_identity(self.root)
        artifact = {
            "package": PACKAGES[0],
            "version": "1.0.0",
            "target": TARGETS[0],
            "archive": "archive.tar.gz",
            "archive_sha256": sha(b"archive"),
            "executable_sha256": sha(b"executable"),
        }
        receipt = {
            "schema_version": 1,
            "kind": "producer-source-lock",
            "signed": False,
            "compiled_dependency_graph": "unknown",
            "source_commit": source["commit"],
            **{
                key: source["cargo_lock_sha256"]
                for key in (
                    "committed_lock_sha256",
                    "before_lock_sha256",
                    "after_lock_sha256",
                )
            },
            **artifact,
            "build": {"job": "producer", "run_id": "123", "run_attempt": "2"},
        }
        env = {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "2"}
        verify_receipt(receipt, artifact, source, env)
        for key in receipt:
            for remove in (True, False):
                changed = copy.deepcopy(receipt)
                if remove:
                    del changed[key]
                else:
                    changed[key] = "wrong"
                with (
                    self.subTest(key=key, remove=remove),
                    self.assertRaises((ValueError, KeyError, TypeError)),
                ):
                    verify_receipt(changed, artifact, source, env)
        for key in ("job", "run_id", "run_attempt"):
            changed = copy.deepcopy(receipt)
            changed["build"][key] = ""
            with self.assertRaises(ValueError):
                verify_receipt(changed, artifact, source, env)
        for key in env:
            with self.assertRaises(ValueError):
                verify_receipt(receipt, artifact, source, {**env, key: "different"})

    def test_all_target_archive_bytes_are_checked(self) -> None:
        for target in TARGETS:
            with self.subTest(target=target):
                dist = self.root / target
                plan = plan_for(target)
                write_archives(dist, target, plan)
                records = archive_records(plan, target, dist, EXPECTED)
                self.assertEqual(len(records), 2)
                path = dist / records[0]["archive"]
                path.write_bytes(path.read_bytes() + b"corrupt")
                with self.assertRaisesRegex(ValueError, "SHA-256 differs"):
                    archive_records(plan, target, dist, EXPECTED)

    def test_each_tag_and_empty_or_partial_plans(self) -> None:
        for target in TARGETS:
            for package in PACKAGES:
                self.assertEqual(
                    len(
                        package_set(
                            plan_for(target, (package,)), target, {package: "1.0.0"}
                        )
                    ),
                    1,
                )
            plan = plan_for(target)
            self.assertEqual(len(package_set(plan, target, EXPECTED)), 2)
            for change in (
                "empty",
                "duplicate",
                "omitted",
                "missing-release",
                "wrong-target",
                "wrong-checksum",
            ):
                bad = copy.deepcopy(plan)
                if change == "empty":
                    bad["releases"] = []
                elif change == "duplicate":
                    bad["releases"].append(bad["releases"][0])
                elif change == "missing-release":
                    bad["releases"].pop()
                elif change == "omitted":
                    bad["releases"][-1]["artifacts"] = []
                else:
                    item = bad["artifacts"][bad["releases"][0]["artifacts"][0]]
                    item[
                        "target_triples" if change == "wrong-target" else "checksum"
                    ] = ["wrong"] if change == "wrong-target" else "wrong"
                with (
                    self.subTest(target=target, change=change),
                    self.assertRaises(ValueError),
                ):
                    package_set(bad, target, EXPECTED)

    def test_cli_failure_retains_a_failing_record(self) -> None:
        manifest = self.root / "manifest.json"
        manifest.write_text('{"releases":[]}')
        output = self.root / "result.json"
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/source_lock.py"),
                "verify",
                native_target(),
                str(manifest),
                str(self.root),
                "--output",
                str(output),
            ],
            cwd=self.root,
            capture_output=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertFalse(json.loads(output.read_text())["verified"])

    def test_suite_scope_comes_from_source_not_plan(self) -> None:
        self.assertEqual(suite_packages(self.root), EXPECTED)
        plan = plan_for(native_target())
        plan["releases"][0]["app_version"] = "9.9.9"
        with self.assertRaisesRegex(ValueError, "suite version differs"):
            package_set(plan, native_target(), EXPECTED)
        crate = self.root / "one/Cargo.toml"
        original = crate.read_text()
        crate.write_text(original.replace("dist=true", "dist=false"))
        self.assertEqual(suite_packages(self.root), {PACKAGES[1]: "1.0.0"})
        with self.assertRaisesRegex(ValueError, "unexpected release package"):
            package_set(
                plan_for(native_target()), native_target(), suite_packages(self.root)
            )
        crate.write_text(original.replace("dist=true", ""))
        with self.assertRaisesRegex(ValueError, "scope must be explicit"):
            suite_packages(self.root)


class WorkflowTests(unittest.TestCase):
    def test_publication_requires_all_successful_dependencies(self) -> None:
        text = (ROOT / ".github/workflows/release.yml").read_text()
        host = text.split("\n  host:\n")[1].split("\n  custom-qualify-packages:")[0]
        dependencies = (
            "plan",
            "build-local-artifacts",
            "build-global-artifacts",
            "custom-qualify-packages",
        )
        for name in dependencies:
            self.assertIn("      - " + name + "\n", host)
        expression = re.search(r"^    if: \$\{\{ (.*) \}\}$", host, re.MULTILINE)[1]
        expression = (
            re.sub(r"needs\.([\w-]+)", r'needs["\1"]', expression)
            .replace("&&", "and")
            .replace("||", "or")
        )
        for dependency in dependencies:
            for status in ("success", "failure", "skipped", "cancelled", "", None):
                for publishing in ("true", "false"):
                    needs = {
                        name: SimpleNamespace(
                            result=status if name == dependency else "success",
                            outputs=SimpleNamespace(publishing=publishing),
                        )
                        for name in dependencies
                    }
                    with self.subTest(
                        dependency=dependency, status=status, publishing=publishing
                    ):
                        self.assertEqual(
                            eval(
                                expression,
                                {
                                    "__builtins__": {},
                                    "needs": needs,
                                    "always": lambda: True,
                                },
                            ),
                            status == "success" and publishing == "true",
                        )

    def test_real_producer_is_guarded_and_signing_is_release_only(self) -> None:
        text = (ROOT / ".github/workflows/release.yml").read_text()
        local = text.split("\n  build-local-artifacts:\n")[1].split(
            "\n  build-global-artifacts:"
        )[0]
        self.assertIn("python scripts/source_lock.py build", local)
        self.assertIn(
            "-- dist build ${{ needs.plan.outputs.tag-flag }} --print=linkage --output-format=json ${{ matrix.dist_args }}",
            local,
        )
        self.assertIn(
            "target/distrib/*-${{ join(matrix.targets, ',') }}-source-lock.json", local
        )
        self.assertIn(
            "name: Build artifacts\n        shell: bash\n        run: |\n          set -euo pipefail",
            local,
        )
        signing = local.split("      - name: Configure macOS signing\n")[1].split(
            "      - name: Build artifacts\n"
        )[0]
        self.assertIn(
            "if: ${{ runner.os == 'macOS' && needs.plan.outputs.publishing == 'true' }}",
            signing,
        )
        self.assertIn("if [ ${#missing[@]} -gt 0 ]; then", signing)
        self.assertIn('echo "CODESIGN_OPTIONS=runtime"', signing)
        self.assertLess(
            local.index("Configure macOS signing"), local.index("source_lock.py build")
        )
        self.assertLess(
            local.index("source_lock.py build"), local.index("Notarise macOS binaries")
        )
        self.assertLess(
            local.index("Notarise macOS binaries"), local.index("source_lock.py verify")
        )
        self.assertLess(
            local.index("source_lock.py verify"),
            local.index("uses: actions/upload-artifact"),
        )
        self.assertIn("codesign --verify", local)
        self.assertIn("xcrun notarytool submit", local)
        self.assertIn("python scripts/test_source_lock.py", local)
        qualifier = text.split("\n  custom-qualify-packages:\n")[1].split(
            "\n  publish-homebrew-formula:"
        )[0]
        for dependency in ("plan", "build-local-artifacts", "build-global-artifacts"):
            self.assertIn("      - " + dependency, qualifier)
        self.assertIn("uses: ./.github/workflows/qualify-packages.yml", qualifier)
        self.assertNotIn("secrets:", qualifier)

    def test_matrix_and_pr_packaging_cover_whole_declared_surface(self) -> None:
        config = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
            "metadata"
        ]["dist"]
        text = (ROOT / ".github/workflows/qualify-packages.yml").read_text()
        matrix = re.findall(r"^            target: ([\w-]+)$", text, re.MULTILINE)
        self.assertEqual(sorted(matrix), sorted(TARGETS))
        self.assertEqual(sorted(config["targets"]), sorted(TARGETS))
        self.assertEqual(config["pr-run-mode"], "upload")
        self.assertEqual(config["global-artifacts-jobs"], ["./qualify-packages"])
        self.assertNotIn("host-jobs", config)
        self.assertIn("python scripts/source_lock.py verify", text)
        self.assertIn("target/distrib/plan-dist-manifest.json", text)
        self.assertNotIn("secrets:", text)
        self.assertIn("uses: ./.github/workflows/test-linux-packages.yml", text)
        linux = (ROOT / ".github/workflows/test-linux-packages.yml").read_text()
        self.assertNotIn("  pull_request:", linux)
        self.assertIn("release_run:", linux)
        self.assertIn("python3 scripts/test-linux-packages.py", linux)
        self.assertIsNone(re.search(r"^\s+PLAN:", linux, re.MULTILINE))
        self.assertGreater(len(suite_packages(ROOT)), 0)


if __name__ == "__main__":
    unittest.main()

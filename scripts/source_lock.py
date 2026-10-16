#!/usr/bin/env python3
"""Bind native package bytes to an unchanged committed producer Cargo.lock."""

from __future__ import annotations

# Adapted from Build198x/build198x/scripts/source_lock.py (producer rollout #137).
import argparse
import hashlib
import io
import json
import os
import platform
import re
import struct
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path
from typing import Any

import tomllib

TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
)


def suite_packages(root: Path) -> dict[str, str]:
    """Read the release surface from source, independently of a dist manifest."""
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    result = {}
    for member in workspace["members"]:
        directory = root / member
        config = tomllib.loads((directory / "Cargo.toml").read_text())
        package = config["package"]
        settings = package.get("metadata", {}).get("dist", {})
        binaries = config.get("bin", [])
        has_main = (directory / "src/main.rs").is_file()
        if settings.get("dist") is False or not (binaries or has_main):
            continue
        require(settings.get("dist") is True, "binary release scope must be explicit")
        name = package["name"]
        require(
            bool(re.fullmatch(r"emu198x-[a-z0-9-]+", name)), "invalid suite package"
        )
        require(
            not binaries or [b["name"] for b in binaries] == [name],
            "expected one package-named executable",
        )
        require(name not in result, "duplicate suite package")
        version = package["version"]
        result[name] = (
            workspace["package"]["version"] if isinstance(version, dict) else version
        )
    require(bool(result), "empty suite release surface")
    return result


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json(path: Path) -> Any:
    require(0 < path.stat().st_size <= 16 * 1024 * 1024, "invalid JSON record size")
    return json.loads(path.read_text())


def source_identity(root: Path) -> dict[str, str]:
    def git(*args: str) -> bytes:
        return subprocess.run(
            ["git", *args], cwd=root, capture_output=True, check=True, timeout=10
        ).stdout

    commit = git("rev-parse", "HEAD").decode().strip()
    require(bool(re.fullmatch(r"[a-f0-9]{40}", commit)), "invalid checkout commit")
    committed = git("show", f"{commit}:Cargo.lock")
    current = (root / "Cargo.lock").read_bytes()
    require(bool(committed.strip()) and bool(current.strip()), "empty producer lock")
    require(current == committed, "producer Cargo.lock differs from checkout commit")
    return {"commit": commit, "cargo_lock_sha256": sha(committed)}


def native_target() -> str:
    machine = platform.machine().lower()
    arch = {
        "arm64": "aarch64",
        "aarch64": "aarch64",
        "x86_64": "x86_64",
        "amd64": "x86_64",
    }.get(machine)
    suffix = {
        "darwin": "apple-darwin",
        "linux": "unknown-linux-gnu",
        "win32": "pc-windows-msvc",
    }.get(sys.platform)
    target = f"{arch}-{suffix}"
    require(target in TARGETS, "unsupported native host")
    return target


def package_set(
    plan: dict[str, Any], target: str, expected: dict[str, str]
) -> list[dict[str, str]]:
    require(target in TARGETS, "unsupported target")
    releases = plan["releases"]
    require(bool(releases), "empty release plan")
    result = []
    for release in releases:
        name = release["app_name"]
        require(name in expected, "unexpected release package")
        version = release["app_version"]
        require(version == expected[name], "suite version differs")
        require(
            bool(re.fullmatch(r"\d+\.\d+\.\d+(?:-[\w.-]+)?", version)),
            "invalid package version",
        )
        extension = "zip" if target.endswith("msvc") else "tar.gz"
        archive = f"{name}-{target}.{extension}"
        require(archive in release["artifacts"], f"plan omits {archive}")
        spec = plan["artifacts"][archive]
        require(spec["target_triples"] == [target], "planned target differs")
        require(spec["checksum"] == archive + ".sha256", "planned checksum differs")
        result.append(
            {"package": name, "version": version, "archive": archive, "target": target}
        )
    require(
        len({p["package"] for p in result}) == len(result), "duplicate release package"
    )
    require(
        {p["package"] for p in result} == set(expected), "plan omits suite packages"
    )
    return result


def binary_target(data: bytes) -> str:
    if data[:4] == b"\x7fELF" and data[4:6] == b"\x02\x01":
        return {62: TARGETS[2], 183: TARGETS[3]}[struct.unpack_from("<H", data, 18)[0]]
    if data[:4] == b"\xcf\xfa\xed\xfe":
        return {0x100000C: TARGETS[0], 0x1000007: TARGETS[1]}[
            struct.unpack_from("<I", data, 4)[0]
        ]
    if data[:2] == b"MZ":
        offset = struct.unpack_from("<I", data, 0x3C)[0]
        require(data[offset : offset + 4] == b"PE\x00\x00", "invalid PE signature")
        require(
            struct.unpack_from("<H", data, offset + 4)[0] == 0x8664,
            "unsupported PE architecture",
        )
        return TARGETS[4]
    raise ValueError("unknown executable architecture")


def archive_records(
    plan: dict[str, Any], target: str, dist: Path, expected: dict[str, str]
) -> list[dict[str, str]]:
    records = []
    for spec in package_set(plan, target, expected):
        path = dist / spec["archive"]
        require(0 < path.stat().st_size <= 128 * 1024 * 1024, "invalid archive size")
        raw = path.read_bytes()
        checksum = (dist / (path.name + ".sha256")).read_text().split()
        require(
            len(checksum) == 2 and checksum[1].lstrip("*") == path.name,
            "checksum filename differs",
        )
        require(checksum[0] == sha(raw), "archive SHA-256 differs")
        if target.endswith("msvc"):
            member = spec["package"] + ".exe"
            with zipfile.ZipFile(io.BytesIO(raw)) as archive:
                require(
                    archive.namelist().count(member) == 1, "expected one executable"
                )
                require(
                    archive.getinfo(member).file_size <= 128 * 1024 * 1024,
                    "oversized executable",
                )
                executable = archive.read(member)
        else:
            member = f"{spec['package']}-{target}/{spec['package']}"
            with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
                members = [m for m in archive.getmembers() if m.name == member]
                require(
                    len(members) == 1 and members[0].isfile(), "expected one executable"
                )
                require(members[0].size <= 128 * 1024 * 1024, "oversized executable")
                stream = archive.extractfile(members[0])
                require(stream is not None, "missing executable")
                executable = stream.read()
        require(binary_target(executable) == target, "executable architecture differs")
        records.append(
            {**spec, "archive_sha256": sha(raw), "executable_sha256": sha(executable)}
        )
    return records


def receipt_name(package: str, target: str) -> str:
    return f"{package}-{target}-source-lock.json"


def verify_receipt(
    receipt: dict[str, Any],
    artifact: dict[str, str],
    source: dict[str, str],
    env: dict[str, str],
) -> None:
    require(
        type(receipt.get("schema_version")) is int
        and receipt["schema_version"] == 1
        and receipt["kind"] == "producer-source-lock",
        "receipt schema differs",
    )
    require(
        receipt["signed"] is False
        and receipt["compiled_dependency_graph"] == "unknown",
        "receipt evidence scope differs",
    )
    require(
        receipt["source_commit"] == source["commit"], "receipt source commit differs"
    )
    for key in ("committed_lock_sha256", "before_lock_sha256", "after_lock_sha256"):
        require(receipt[key] == source["cargo_lock_sha256"], f"receipt {key} differs")
    for key, value in artifact.items():
        require(receipt[key] == value, f"receipt {key} differs")
    if env.get("GITHUB_ACTIONS") == "true":
        require(
            receipt["build"]["job"] == "build-local-artifacts",
            "receipt producer job differs",
        )
    for key in ("job", "run_id", "run_attempt"):
        require(
            isinstance(receipt["build"][key], str) and bool(receipt["build"][key]),
            f"missing producer {key}",
        )
    require(
        receipt["build"]["run_id"] == env.get("GITHUB_RUN_ID", "local"),
        "receipt producer run differs",
    )
    require(
        receipt["build"]["run_attempt"] == env.get("GITHUB_RUN_ATTEMPT", "local"),
        "receipt producer attempt differs",
    )


def guarded_command(root: Path, argv: list[str], manifest: Path) -> dict[str, str]:
    before = source_identity(root)
    require(bool(argv), "missing packaging command")
    manifest.parent.mkdir(parents=True, exist_ok=True)
    with manifest.open("wb") as output:
        subprocess.run(argv, cwd=root, stdout=output, check=True, timeout=3600)
    require(source_identity(root) == before, "source changed during packaging")
    return before


def build(
    root: Path, target: str, manifest: Path, dist: Path, argv: list[str]
) -> list[dict[str, Any]]:
    require(native_target() == target, "packaging must use native host")
    expected = suite_packages(root)
    for name in expected:
        require(
            not (dist / receipt_name(name, target)).exists(),
            "stale producer receipt exists",
        )
    source = guarded_command(root, argv, manifest)
    receipts = []
    for artifact in archive_records(
        read_json(manifest), target, dist, suite_packages(root)
    ):
        receipt = {
            "schema_version": 1,
            "kind": "producer-source-lock",
            "signed": False,
            "source_commit": source["commit"],
            "committed_lock_sha256": source["cargo_lock_sha256"],
            "before_lock_sha256": source["cargo_lock_sha256"],
            "after_lock_sha256": source["cargo_lock_sha256"],
            **artifact,
            "build": {
                "job": os.environ.get("GITHUB_JOB", "local"),
                "run_id": os.environ.get("GITHUB_RUN_ID", "local"),
                "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "local"),
            },
            "command": argv,
            "compiled_dependency_graph": "unknown",
        }
        verify_receipt(receipt, artifact, source, dict(os.environ))
        with (dist / receipt_name(artifact["package"], target)).open("x") as output:
            output.write(json.dumps(receipt, indent=2) + "\n")
        receipts.append(receipt)
    return receipts


def verify(root: Path, target: str, manifest: Path, dist: Path) -> dict[str, Any]:
    require(native_target() == target, "verification must use native host")
    source = source_identity(root)
    receipts = []
    for artifact in archive_records(
        read_json(manifest), target, dist, suite_packages(root)
    ):
        receipt = read_json(dist / receipt_name(artifact["package"], target))
        verify_receipt(receipt, artifact, source, dict(os.environ))
        receipts.append(receipt)
    require(bool(receipts), "zero artifacts verified")
    return {
        "schema_version": 1,
        "kind": "producer-source-lock-verification",
        "verified": True,
        "scope": "producer-source-lock",
        "source": source,
        "target": target,
        "receipts": receipts,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    for operation in ("build", "verify"):
        command = sub.add_parser(operation)
        command.add_argument("target", choices=TARGETS)
        command.add_argument("manifest", type=Path)
        command.add_argument("dist", type=Path)
        if operation == "build":
            command.add_argument("command", nargs=argparse.REMAINDER)
        else:
            command.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.operation == "build":
            argv = args.command[1:] if args.command[:1] == ["--"] else args.command
            build(
                Path.cwd(),
                args.target,
                args.manifest.resolve(),
                args.dist.resolve(),
                argv,
            )
        else:
            report = verify(Path.cwd(), args.target, args.manifest, args.dist)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + "\n")
        return 0
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        struct.error,
        subprocess.SubprocessError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as exc:
        if args.operation == "verify":
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(
                json.dumps({"verified": False, "error": str(exc)}) + "\n"
            )
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

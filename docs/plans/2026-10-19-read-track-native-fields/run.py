#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Fresh source-built native-state checks. No ROM loading qualification."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import subprocess
import sys
import tarfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FILTER = "snapshot::tests::read_track_native_field_matrix"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    out = HERE / "runs"
    out.mkdir(exist_ok=False)
    record = {
        "schema_version": 1,
        "qualified": False,
        "evidence_class": "fresh_execution",
        "kind": "native-v6-read-track-field-preservation",
        "host": platform.platform(),
        "configuration": {
            "runtime": "SpectrumPlus3Runtime::blank()",
            "firmware": "not applicable; zero-filled blank machine",
            "host": "direct FDC Peripheral ticks through existing test helpers; no guest execution",
            "snapshot_version": 6,
        },
        "bounds": {
            "host_seconds_per_execution": 120,
            "ticks_per_byte_wait": 10000,
            "max_payload_bytes": 16384,
            "result_bytes": 7,
        },
        "commands": [],
        "executions": [],
    }

    def save() -> None:
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")

    def command(
        name: str, args: list, timeout: int = 120, extra: dict | None = None
    ) -> tuple[dict, bytes, bytes]:
        env = {"LC_ALL": "C", "TZ": "UTC", **(extra or {})}
        start = time.monotonic()
        row = {
            "name": name,
            "argv": list(map(str, args)),
            "cwd": str(REPO),
            "environment": env,
            "host_timeout_seconds": timeout,
        }
        try:
            p = subprocess.run(
                row["argv"],
                cwd=REPO,
                env={**os.environ, **env},
                capture_output=True,
                timeout=timeout,
                check=False,
            )
            stdout, stderr = p.stdout, p.stderr
            row.update(returncode=p.returncode, timed_out=False)
        except subprocess.TimeoutExpired as e:
            stdout, stderr = e.stdout or b"", e.stderr or b""
            row.update(returncode=None, timed_out=True)
        (out / (name + ".stdout")).write_bytes(stdout)
        (out / (name + ".stderr")).write_bytes(stderr)
        row["host_seconds"] = time.monotonic() - start
        record["commands"].append(row)
        save()
        if len(stdout) + len(stderr) > 16 * 1024 * 1024:
            raise ValueError("bounded test log budget exceeded")
        return row, stdout, stderr

    record["source"] = {
        "base_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "files": {
            name: sha(REPO / name)
            for name in (
                "crates/runtime-sinclair-zx-spectrum/src/snapshot.rs",
                "crates/nec-upd765a/src/lib.rs",
                "crates/format-amstrad-dsk/src/lib.rs",
                "Cargo.lock",
                "rust-toolchain.toml",
            )
        },
    }
    record["recipe_sha256"] = {
        str(p.relative_to(REPO)): sha(p)
        for p in sorted(HERE.iterdir())
        if p.is_file() and p.suffix in (".py", ".md")
    }
    with tarfile.open(out / "capture-source.tar.gz", "w:gz") as archive:
        for name in {**record["source"]["files"], **record["recipe_sha256"]}:
            archive.add(REPO / name, arcname=name, recursive=False)
    record["tools"] = {}
    for name in ("rustc", "cargo"):
        path = Path(
            subprocess.check_output(["rustup", "which", name], text=True).strip()
        )
        row, stdout, _ = command(name + "-version", [path, "-Vv"])
        if row["returncode"] != 0:
            raise RuntimeError(name + " version failed")
        record["tools"][name] = {
            "path": str(path),
            "sha256": sha(path),
            "version": stdout.decode(),
        }
    path = Path(sys.executable).resolve()
    record["tools"]["python"] = {
        "path": str(path),
        "sha256": sha(path),
        "version": sys.version,
    }
    row, stdout, stderr = command(
        "build",
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "runtime-sinclair-zx-spectrum",
            "--lib",
            "read_track_native_field_matrix",
            "--no-run",
            "--message-format=json",
        ],
        300,
        {
            "CARGO_TARGET_DIR": str(HERE / "target"),
            "CARGO_INCREMENTAL": "0",
            "CARGO_PROFILE_DEV_DEBUG": "0",
        },
    )
    if row["returncode"] != 0:
        raise RuntimeError(
            "native test build failed: " + stderr.decode(errors="replace")
        )
    records = [
        json.loads(line) for line in stdout.splitlines() if line.startswith(b"{")
    ]
    binary = Path(next(x["executable"] for x in records if x.get("executable")))
    record["executable"] = {
        "path": str(binary),
        "sha256": sha(binary),
        "delivery": "source_build",
    }
    save()
    for repeat in (1, 2):
        artifacts = out / f"repeat-{repeat}"
        artifacts.mkdir()
        row, stdout, stderr = command(
            f"positive-{repeat}",
            [binary, FILTER, "--exact", "--nocapture", "--test-threads=1"],
            extra={"READ_TRACK_NATIVE_ARTIFACTS": str(artifacts)},
        )
        if row["returncode"] != 0:
            raise RuntimeError("native checkpoint test failed; see captured logs")
        observations = []
        summary = None
        for line in stdout.decode().splitlines():
            if "READ_TRACK_NATIVE " in line:
                observations.append(json.loads(line.split("READ_TRACK_NATIVE ", 1)[1]))
            if "READ_TRACK_NATIVE_SUMMARY " in line:
                summary = json.loads(line.split("READ_TRACK_NATIVE_SUMMARY ", 1)[1])
        if (
            summary
            != {
                "schema_version": 1,
                "qualified": False,
                "cases": 13,
                "checkpoints": 180,
            }
            or len(observations) != 180
        ):
            raise ValueError("missing native assertions/observations")
        record["executions"].append(
            {
                "repeat": repeat,
                "status": "pass",
                "summary": summary,
                "observations": observations,
            }
        )
        save()
    artifacts = out / "wrong-expected"
    artifacts.mkdir()
    row, stdout, stderr = command(
        "wrong-expected",
        [binary, FILTER, "--exact", "--nocapture", "--test-threads=1"],
        extra={
            "READ_TRACK_NATIVE_ARTIFACTS": str(artifacts),
            "READ_TRACK_NATIVE_WRONG_EXPECTED": "1",
        },
    )
    intended = (
        not row["timed_out"]
        and row["returncode"] != 0
        and b"restored continuation differs from expected payload" in stdout + stderr
    )
    record["wrong_expected"] = {
        "status": "pass" if intended else "fail",
        "process_returncode": row["returncode"],
        "meaning": "intended negative expectation rejected; not a passing runtime execution",
    }
    record["status"] = "captured" if intended else "fail"
    save()
    if not intended:
        raise ValueError("wrong continuation did not fail for the intended assertion")
    files = {
        str(p.relative_to(out)): sha(p)
        for p in sorted(out.rglob("*"))
        if p.is_file() and p.name != "hashes.json"
    }
    (out / "hashes.json").write_text(json.dumps(files, indent=2) + "\n")


if __name__ == "__main__":
    main()

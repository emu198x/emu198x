#!/usr/bin/env python3
"""Unmodified Fuse source-controller observations; never ROM qualification."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FUSE = Path("/Users/stevehill/Projects/198x/emulators/zx-spectrum/fuse-emulator-fuse")


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> None:
    out = HERE / "runs"
    out.mkdir(exist_ok=False)
    record = {
        "schema_version": 1,
        "qualified": False,
        "origin": "fresh_execution",
        "kind": "source-controller-register-investigation",
        "host": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "cases": [],
    }

    def run(name: str, argv: list[str], timeout: int = 15):
        begin = time.monotonic()
        p = subprocess.run(
            argv,
            cwd=HERE,
            env={**os.environ, "LC_ALL": "C", "TZ": "UTC"},
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        (out / f"{name}.stdout").write_bytes(p.stdout)
        (out / f"{name}.stderr").write_bytes(p.stderr)
        record["commands"].append(
            {
                "name": name,
                "argv": argv,
                "cwd": str(HERE),
                "environment": {"LC_ALL": "C", "TZ": "UTC"},
                "timeout_seconds": timeout,
                "returncode": p.returncode,
                "elapsed_seconds": round(time.monotonic() - begin, 3),
            }
        )
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")
        p.check_returncode()
        return p

    commit = subprocess.check_output(
        ["git", "-C", str(FUSE), "rev-parse", "HEAD"], text=True
    ).strip()
    assert commit == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    assert not subprocess.check_output(
        ["git", "-C", str(FUSE), "status", "--porcelain", "--untracked-files=no"],
        text=True,
    ).strip()
    clang = Path(shutil.which("clang")).resolve()
    record["compiler"] = {
        "path": str(clang),
        "sha256": sha(clang),
        "version": run("clang-version", [str(clang), "--version"]).stdout.decode(),
    }
    paths = [
        "event.h",
        "bitmap.h",
        *[
            f"peripherals/disk/{x}"
            for x in [
                "upd_fdc.c",
                "upd_fdc.h",
                "fdd.c",
                "fdd.h",
                "disk.h",
                "crc.c",
                "crc.h",
            ]
        ],
    ]
    record["fuse_source"] = {
        "commit": commit,
        "patched": False,
        "files": {p: sha(FUSE / p) for p in paths},
    }
    record["authored_sources"] = {
        str(p.relative_to(HERE)): sha(p)
        for p in [
            HERE / "run.py",
            HERE / "host.c",
            HERE / "fuse_probe.c",
            *sorted((HERE / "host").glob("*")),
        ]
        if p.is_file()
    }
    binary = HERE / "fuse-stock"
    record["compiler_argv"] = [
        str(clang),
        "-std=c11",
        "-O1",
        "-g",
        "-fsanitize=address,undefined",
        "-I" + str(HERE / "host"),
        "-I" + str(FUSE),
        "-I" + str(FUSE / "peripherals/disk"),
        "-include",
        str(HERE / "host/config.h"),
        str(HERE / "host.c"),
        str(HERE / "fuse_probe.c"),
        str(FUSE / "peripherals/disk/upd_fdc.c"),
        str(FUSE / "peripherals/disk/fdd.c"),
        str(FUSE / "peripherals/disk/crc.c"),
        "-o",
        str(binary),
    ]
    run("build", record["compiler_argv"], 60)
    record["executable"] = {
        "path": str(binary),
        "sha256": sha(binary),
        "delivery": "source_build",
    }
    cases = [(0x42, n, mask) for mask in [0, 1, 2, 3] for n in [0, 1, 17, 512]]
    for repeat in [1, 2]:
        for op, n, mask in cases:
            name = f"{repeat}-{op:02x}-{n}-crc{mask}"
            raw = out / f"{name}.raw"
            p = run(name, [str(binary), str(op), str(n), str(mask), str(raw)])
            assert not p.stderr, "reference/sanitizer diagnostic"
            observed = json.loads(p.stdout)
            assert (
                observed["consumed"] == n
                and observed["idle_msr"] == 128
                and observed["media_unchanged"]
            )
            assert len(observed["result"]) == 7
            record["cases"].append(
                {
                    "repeat": repeat,
                    "opcode": op,
                    "consumed": n,
                    "crc_mask": mask,
                    "raw_path": raw.name,
                    "raw_sha256": sha(raw),
                    "observed": observed,
                }
            )
            (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")
    record["checks_passed"] = True
    (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")
    (out / "hashes.json").write_text(
        json.dumps(
            {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file()}, indent=2
        )
        + "\n"
    )
    print(
        json.dumps(
            {
                "observations": len(record["cases"]),
                "qualified": False,
                "checks_passed": True,
            }
        )
    )


if __name__ == "__main__":
    main()

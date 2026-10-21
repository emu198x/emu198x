#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Bounded fresh reference Seek timing, not hardware/ROM-loader qualification."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from media import edsk

HERE = Path(__file__).resolve().parent
FAMILY = Path("/Users/stevehill/Projects/198x")
FUSE = FAMILY / "emulators/zx-spectrum/fuse-emulator-fuse"


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> None:
    out = HERE / "runs"
    out.mkdir(exist_ok=False)
    record: dict = {
        "schema_version": 1,
        "qualified": False,
        "status": "not_run",
        "origin": "fresh_execution",
        "class": "standalone-controller-and-direct-port-reference-research",
        "host": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "observations": [],
    }

    def save() -> None:
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")

    def run(
        name: str, argv: list[str], timeout: int = 15, env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess:
        override = {"LC_ALL": "C", "TZ": "UTC", **(env or {})}
        start = time.monotonic()
        result = subprocess.run(
            argv,
            cwd=HERE,
            env={**os.environ, **override},
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        (out / f"{name}.stdout").write_bytes(result.stdout)
        (out / f"{name}.stderr").write_bytes(result.stderr)
        record["commands"].append(
            {
                "name": name,
                "argv": argv,
                "cwd": str(HERE),
                "environment": override,
                "host_timeout_seconds": timeout,
                "exit": result.returncode,
                "elapsed_seconds": time.monotonic() - start,
            }
        )
        save()
        result.check_returncode()
        return result

    commit = subprocess.check_output(
        ["git", "-C", str(FUSE), "rev-parse", "HEAD"], text=True
    ).strip()
    assert commit == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    assert not subprocess.check_output(
        ["git", "-C", str(FUSE), "status", "--porcelain", "--untracked-files=no"],
        text=True,
    ).strip()
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
                "crc.c",
                "crc.h",
                "disk.h",
            ]
        ],
    ]
    record["fuse_source"] = {
        "commit": commit,
        "modified": False,
        "files": {p: sha(FUSE / p) for p in paths},
    }
    record["authored_sources"] = {
        str(p.relative_to(HERE)): sha(p)
        for p in HERE.rglob("*")
        if p.is_file() and "runs" not in p.parts and "__pycache__" not in p.parts
    }
    clang = Path(shutil.which("clang") or "/missing/clang").resolve()
    mame = Path(shutil.which("mame") or "/missing/mame").resolve()
    record["compiler"] = {
        "path": str(clang),
        "sha256": sha(clang),
        "version": run("clang-version", [str(clang), "--version"]).stdout.decode(),
    }
    sdk = run("sdk", ["xcrun", "--show-sdk-path"]).stdout.decode().strip()
    binary = out / "fuse-stock"
    args = [
        str(clang),
        "-std=c11",
        "-O1",
        "-g",
        "-fsanitize=address,undefined",
        "-isysroot",
        sdk,
        "-I" + str(HERE / "host"),
        "-I" + str(FUSE),
        "-I" + str(FUSE / "peripherals/disk"),
        "-include",
        str(HERE / "host/config.h"),
        str(HERE / "host.c"),
        str(HERE / "fuse_probe.c"),
        *[str(FUSE / "peripherals/disk" / x) for x in ["upd_fdc.c", "fdd.c", "crc.c"]],
        "-o",
        str(binary),
    ]
    run("fuse-build", args, 60)
    record["fuse_executable"] = {
        "path": str(binary),
        "sha256": sha(binary),
        "delivery": "source_build",
    }
    record["mame_executable"] = {
        "path": str(mame),
        "sha256": sha(mame),
        "delivery": "installed_binary",
        "source_commit": None,
        "version": run("mame-version", [str(mame), "-help"])
        .stdout.decode()
        .splitlines()[0],
    }
    firmware = [
        (
            Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        ).read_bytes()
        for i in range(4)
    ]
    assert all(len(x) == 16384 for x in firmware)
    record["firmware_sha256"] = [hashlib.sha256(x).hexdigest() for x in firmware]
    disk = out / "authored.dsk"
    disk.write_bytes(edsk(False, False))
    digest = sha(disk)
    for repeat in (1, 2):
        for srt in (0, 1, 14, 15):
            name = f"fuse-{repeat}-{srt}"
            result = run(name, [str(binary), str(srt)])
            assert not result.stderr
            rows = [json.loads(x) for x in result.stdout.splitlines()]
            assert len(rows) == 4
            record["observations"].append(
                {
                    "engine": "fuse",
                    "repeat": repeat,
                    "srt": srt,
                    "raw": f"{name}.stdout",
                    "rows": rows,
                }
            )
            save()
    with tempfile.TemporaryDirectory(prefix="198x-seek-private-") as temp:
        private = Path(temp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(firmware[0] + firmware[1])
        (roms / "40093.ic8").write_bytes(firmware[2] + firmware[3])
        for repeat, poll in ((1, 1), (2, 32)):
            name = f"mame-{repeat}"
            observation = private / "observation.jsonl"
            observation.unlink(missing_ok=True)
            argv = [
                str(mame),
                "specpls3",
                "-bios",
                "en",
                "-rompath",
                str(private),
                "-flop1",
                str(disk),
                "-video",
                "none",
                "-sound",
                "none",
                "-nothrottle",
                "-skip_gameinfo",
                "-noreadconfig",
                "-nowriteconfig",
                "-cfg_directory",
                str(private / "cfg"),
                "-autoboot_delay",
                "0",
                "-autoboot_script",
                str(HERE / "mame-probe.lua"),
                "-seconds_to_run",
                "10",
            ]
            run(
                name,
                argv,
                45,
                {
                    "SDL_VIDEODRIVER": "dummy",
                    "SDL_AUDIODRIVER": "dummy",
                    "PROBE_OUTPUT": str(observation),
                    "PROBE_POLL_US": str(poll),
                },
            )
            raw = observation.read_bytes()
            (out / f"{name}.jsonl").write_bytes(raw)
            rows = [json.loads(x) for x in raw.splitlines()]
            assert len(rows) == 16
            record["observations"].append(
                {
                    "engine": "mame",
                    "repeat": repeat,
                    "poll_us": poll,
                    "raw": f"{name}.jsonl",
                    "rows": rows,
                }
            )
            save()
    assert sha(disk) == digest
    record.update(
        {
            "status": "observed",
            "capture_completed": True,
            "source_media_unchanged": True,
        }
    )
    save()
    (out / "hashes.json").write_text(
        json.dumps({p.name: sha(p) for p in out.iterdir() if p.is_file()}, indent=2)
        + "\n"
    )
    print(
        "Captured8 Fuse processes and2 MAME processes;64 operation observations; qualified=false"
    )


if __name__ == "__main__":
    main()

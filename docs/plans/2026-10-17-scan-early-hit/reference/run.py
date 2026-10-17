#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Capture bounded controller probes, with stock and patched references separate."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
SPEC = importlib.util.spec_from_file_location(
    "prior_probe", HERE.parents[1] / "2026-10-17-fdc-commands/reference/run.py"
)
assert SPEC and SPEC.loader
prior = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prior)
FUSE = "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"


def sha(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fuse-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = args.fuse_root.resolve()
    revision = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    assert revision == FUSE, "wrong Fuse source revision"
    assert not subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"],
        text=True,
    ).strip()
    record = {
        "schema_version": 1,
        "qualified": False,
        "kind": "controller-register-probes-not-rom-or-hardware-qualification",
        "evidence_class": "fresh_execution",
        "host": {"system": platform.platform(), "python": platform.python_version()},
        "environment": {"LC_ALL": "C", "TZ": "UTC"},
        "source_commit": subprocess.check_output(
            ["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True
        ).strip(),
        "sources": {},
        "commands": [],
        "fuse": [],
        "mame": [],
        "emu": [],
    }
    env = {**os.environ, **record["environment"]}

    def execute(
        name: str,
        argv: list[str],
        timeout: int,
        extra: dict | None = None,
        cwd: Path | None = None,
    ):
        start = time.monotonic()
        p = subprocess.run(
            argv,
            env={**env, **(extra or {})},
            cwd=cwd,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        for suffix, data in [("stdout", p.stdout), ("stderr", p.stderr)]:
            (out / f"{name}.{suffix}").write_bytes(data)
        record["commands"].append(
            {
                "name": name,
                "argv": argv,
                "returncode": p.returncode,
                "host_seconds": round(time.monotonic() - start, 3),
                "timeout_seconds": timeout,
                "environment": extra or {},
                "cwd": str(cwd or REPO),
            }
        )
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")
        p.check_returncode()
        return p

    compiler = Path(shutil.which("clang") or "/usr/bin/clang").resolve()
    record["compiler"] = {
        "path": str(compiler),
        "sha256": sha(compiler),
        "version": execute(
            "clang-version", [str(compiler), "--version"], 10
        ).stdout.decode(),
    }
    for path in [
        *HERE.glob("*.c"),
        *HERE.glob("*.lua"),
        *HERE.glob("*.patch"),
        *HERE.glob("*.py"),
        *sorted((HERE / "host").glob("*")),
    ]:
        if path.is_file():
            record["sources"][str(path.relative_to(HERE))] = sha(path)
    paths = [
        "event.h",
        "bitmap.h",
        *[
            "peripherals/disk/" + p
            for p in [
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
        "commit": FUSE,
        "files": {p: sha(source / p) for p in paths},
        "patched": False,
    }
    with tempfile.TemporaryDirectory(prefix="198x-scan-build-") as temp:
        build = Path(temp)
        for label in ["stock", "patched"]:
            unit = source / "peripherals/disk/upd_fdc.c"
            if label == "patched":
                unit = build / "upd_fdc.c"
                shutil.copyfile(source / "peripherals/disk/upd_fdc.c", unit)
                execute(
                    "patch-fuse",
                    [
                        "patch",
                        "--batch",
                        str(unit),
                        str(HERE / "fuse-continuation.patch"),
                    ],
                    10,
                )
            binary = build / f"fuse-{label}"
            argv = [
                str(compiler),
                "-std=c11",
                "-O1",
                "-g",
                "-fsanitize=address,undefined",
                "-I" + str(HERE / "host"),
                "-I" + str(source),
                "-I" + str(source / "peripherals/disk"),
                "-include",
                str(HERE / "host/config.h"),
                str(HERE / "host.c"),
                str(HERE / "fuse_probe.c"),
                str(unit),
                str(source / "peripherals/disk/fdd.c"),
                str(source / "peripherals/disk/crc.c"),
                "-o",
                str(binary),
            ]
            execute("build-" + label, argv, 60)
            identity = {
                "path": str(binary),
                "sha256": sha(binary),
                "controller_sha256": sha(unit),
                "delivery": "source_build",
                "patch": label == "patched",
            }
            for repeat in [1, 2]:
                for case, host in [
                    ("first-hit", 33),
                    ("later-hit", 34),
                    ("no-hit", 35),
                    ("bad-crc", 33),
                ]:
                    name = f"fuse-{label}-{repeat}-{case}"
                    track = out / (name + ".raw")
                    argv = [str(binary), str(host), str(track)] + (
                        ["bad-crc"] if case == "bad-crc" else []
                    )
                    p = execute(name, argv, 10)
                    assert not p.stderr, "sanitizer/reference diagnostic"
                    record["fuse"].append(
                        {
                            "case": case,
                            "repeat": repeat,
                            "runtime": label,
                            "identity": identity,
                            "observation": json.loads(p.stdout),
                            "track": track.name,
                            "track_sha256": sha(track),
                        }
                    )
    mame = Path(shutil.which("mame") or "").resolve()
    assert mame.is_file(), "missing prerequisite: MAME"
    record["mame_identity"] = {
        "path": str(mame),
        "sha256": sha(mame),
        "delivery": "installed_binary",
        "version": execute("mame-version", [str(mame), "-help"], 10)
        .stdout.decode()
        .splitlines()[0],
    }
    pages = [
        (
            Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        ).read_bytes()
        for i in range(4)
    ]
    record["firmware_sha256"] = [hashlib.sha256(p).hexdigest() for p in pages]
    with tempfile.TemporaryDirectory(prefix="198x-scan-roms-") as temp:
        private = Path(temp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0] + pages[1])
        (roms / "40093.ic8").write_bytes(pages[2] + pages[3])
        for repeat in [1, 2]:
            for case, host in [("first-hit", 33), ("later-hit", 34), ("no-hit", 35)]:
                name = f"mame-{repeat}-{case}"
                media = out / (name + ".dsk")
                media.write_bytes(prior.disk([1, 2]))
                observation = private / "observation.json"
                observation.unlink(missing_ok=True)
                extra = {
                    "SDL_VIDEODRIVER": "dummy",
                    "SDL_AUDIODRIVER": "dummy",
                    "PROBE_OUTPUT": str(observation),
                    "PROBE_OPCODE": "81",
                    "PROBE_FILL": str(host),
                    "PROBE_R": "1",
                    "PROBE_EOT": "2",
                    "PROBE_STP": "1",
                }
                argv = [
                    str(mame),
                    "specpls3",
                    "-bios",
                    "en",
                    "-rompath",
                    str(private),
                    "-flop1",
                    str(media),
                    "-video",
                    "none",
                    "-sound",
                    "none",
                    "-nothrottle",
                    "-skip_gameinfo",
                    "-noreadconfig",
                    "-nowriteconfig",
                    "-autoboot_delay",
                    "0",
                    "-autoboot_script",
                    str(HERE / "mame-probe.lua"),
                    "-seconds_to_run",
                    "10",
                ]
                digest = sha(media)
                execute(name, argv, 45, extra, private)
                assert sha(media) == digest
                value = json.loads(observation.read_text())
                record["mame"].append(
                    {
                        "case": case,
                        "repeat": repeat,
                        "observation": value,
                        "media_sha256": digest,
                        "media_unchanged": True,
                    }
                )
    target = out.parent / "cargo-target"
    execute(
        "controller-tests",
        ["cargo", "test", "--locked", "-p", "nec-upd765a"],
        180,
        {"CARGO_TARGET_DIR": str(target)},
        REPO,
    )
    execute(
        "controller-clippy",
        [
            "cargo",
            "clippy",
            "--locked",
            "-p",
            "nec-upd765a",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        180,
        {"CARGO_TARGET_DIR": str(target)},
        REPO,
    )
    for repeat in [1, 2]:
        name = f"emu-{repeat}"
        argv = [
            "cargo",
            "test",
            "--locked",
            "-p",
            "nec-upd765a",
            "--test",
            "scan",
            "scan_equal_counts_first_later_and_absent_hits_without_mutating_media",
            "--",
            "--exact",
            "--nocapture",
        ]
        p = execute(name, argv, 180, {"CARGO_TARGET_DIR": str(target)}, REPO)
        rows = [
            json.loads(line.split("SCAN_OBSERVATION ", 1)[1])
            for line in p.stdout.decode().splitlines()
            if "SCAN_OBSERVATION " in line
        ]
        assert len(rows) == 3, "zero/missing executed assertions"
        record["emu"].append({"repeat": repeat, "observations": rows})
    binaries = [
        p
        for p in (target / "debug/deps").glob("scan-*")
        if p.is_file() and p.suffix == ""
    ]
    assert len(binaries) == 1
    record["emu_identity"] = {
        "path": str(binaries[0]),
        "sha256": sha(binaries[0]),
        "delivery": "source_test_build",
        "controller_sha256": sha(REPO / "crates/nec-upd765a/src/lib.rs"),
        "test_sha256": sha(REPO / "crates/nec-upd765a/tests/scan.rs"),
        "cargo_lock_sha256": sha(REPO / "Cargo.lock"),
    }
    for label, argv in [("rustc", ["rustc", "-Vv"]), ("cargo", ["cargo", "-V"])]:
        record[label] = execute(label, argv, 10).stdout.decode()
    record["artifacts"] = [
        {"path": str(p.relative_to(out)), "sha256": sha(p), "size": p.stat().st_size}
        for p in sorted(out.iterdir())
        if p.is_file() and p.name != "campaign.json"
    ]
    (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")
    print("Captured comparisons; qualified=false:", out / "campaign.json")


if __name__ == "__main__":
    main()

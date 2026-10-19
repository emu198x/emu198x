#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Fresh bounded controller study. Direct register host, never ROM qualification."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path

from cases import input_text, matrix, raw_track

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FAMILY = Path("/Users/stevehill/Projects/198x")
PRIOR = HERE.parent / "2026-10-19-read-track-drained-crc"
RAW = HERE.parent / "2026-10-19-read-track-raw-crc"
FUSE = FAMILY / "emulators/zx-spectrum/fuse-emulator-fuse"
MAME_SOURCE = FAMILY / "emulators/multi-system/mame"


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ValueError("missing helper")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main() -> None:
    out = HERE / "runs"
    out.mkdir(exist_ok=False)
    record = {
        "schema_version": 1,
        "qualified": False,
        "kind": "read-track-field-boundaries",
        "evidence_class": "fresh_execution",
        "configuration": {
            "machine": "Amstrad ZX Spectrum +3 English",
            "controller": "NEC uPD765A",
            "drive": "internal 3-inch single-sided double-density",
            "raw_track_bytes": 6250,
            "bit_rate": 250000,
            "scope": "direct registers; no ROM loading or guest execution qualification",
        },
        "host": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "observations": [],
        "cases": matrix(),
    }

    def save() -> None:
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")

    def run(
        name: str,
        args: list,
        timeout: int = 30,
        extra: dict[str, str] | None = None,
        cwd: Path = HERE,
    ) -> tuple[dict, bytes, bytes]:
        env = {"LC_ALL": "C", "TZ": "UTC", **(extra or {})}
        start = time.monotonic()
        row = {
            "name": name,
            "argv": list(map(str, args)),
            "cwd": str(cwd),
            "environment": env,
            "host_timeout_seconds": timeout,
        }
        try:
            p = subprocess.run(
                row["argv"],
                cwd=cwd,
                env={**os.environ, **env},
                capture_output=True,
                timeout=timeout,
                check=False,
            )
            row.update(returncode=p.returncode, timed_out=False)
            stdout, stderr = p.stdout, p.stderr
        except subprocess.TimeoutExpired as e:
            stdout, stderr = e.stdout or b"", e.stderr or b""
            row.update(returncode=None, timed_out=True)
        (out / f"{name}.stdout").write_bytes(stdout)
        (out / f"{name}.stderr").write_bytes(stderr)
        row["host_seconds"] = time.monotonic() - start
        record["commands"].append(row)
        save()
        return row, stdout, stderr

    def checked(name: str, args: list, **kw) -> bytes:
        row, stdout, stderr = run(name, args, **kw)
        if row["returncode"] != 0:
            raise RuntimeError(f"{name}: {row}; {stderr.decode(errors='replace')}")
        return stdout

    record["emu_source"] = {
        "base_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "controller_sha256": sha(REPO / "crates/nec-upd765a/src/lib.rs"),
        "probe_sha256": sha(REPO / "crates/nec-upd765a/tests/read_track_fields.rs"),
    }
    paths = [
        *sorted(HERE.glob("*.py")),
        *sorted(HERE.glob("*.lua")),
        *sorted(HERE.glob("*.c")),
        HERE / "plan.md",
        PRIOR / "host.c",
        *sorted((PRIOR / "host").glob("*")),
        RAW / "mfi.py",
        RAW / "decode.py",
    ]
    record["recipe_sha256"] = {
        str(p.relative_to(REPO)): sha(p) for p in paths if p.is_file()
    }
    save()
    required = [
        Path("/opt/homebrew/Cellar/mame/0.289/bin/mame"),
        *[
            (Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom")
            for i in range(4)
        ],
    ]
    missing = [str(p) for p in required if not p.is_file()]
    if missing:
        record["status"] = "missing_prerequisite"
        record["missing"] = missing
        save()
        return
    captured_sources = [
        *record["recipe_sha256"],
        "crates/nec-upd765a/tests/read_track_fields.rs",
        "crates/nec-upd765a/src/lib.rs",
        "Cargo.lock",
        "rust-toolchain.toml",
    ]
    with tarfile.open(out / "capture-source.tar.gz", "w:gz") as archive:
        for name in captured_sources:
            archive.add(REPO / name, arcname=name, recursive=False)
    fuse_commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=FUSE, text=True
    ).strip()
    if subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=no"], cwd=FUSE, text=True
    ).strip():
        raise ValueError("dirty Fuse source")
    record["fuse_source"] = {
        "commit": fuse_commit,
        "modified": False,
        "files": {
            str(p.relative_to(FUSE)): sha(p)
            for p in [
                FUSE / "peripherals/disk" / s
                for s in (
                    "upd_fdc.c",
                    "upd_fdc.h",
                    "fdd.c",
                    "fdd.h",
                    "crc.c",
                    "crc.h",
                    "disk.h",
                )
            ]
        },
    }
    clang = Path(shutil.which("clang")).resolve()
    record["clang"] = {
        "path": str(clang),
        "sha256": sha(clang),
        "version": checked("clang-version", [clang, "--version"]).decode(),
    }
    binary = HERE / "fuse-stock"
    checked(
        "fuse-build",
        [
            clang,
            "-std=c11",
            "-O1",
            "-g",
            "-fsanitize=address,undefined",
            "-I" + str(PRIOR / "host"),
            "-I" + str(FUSE),
            "-I" + str(FUSE / "peripherals/disk"),
            "-include",
            PRIOR / "host/config.h",
            PRIOR / "host.c",
            HERE / "fuse_probe.c",
            *[FUSE / "peripherals/disk" / n for n in ("upd_fdc.c", "fdd.c", "crc.c")],
            "-o",
            binary,
        ],
        timeout=60,
    )
    record["fuse_executable"] = {
        "path": str(binary),
        "sha256": sha(binary),
        "delivery": "source_build",
    }
    build = checked(
        "emu-build",
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "nec-upd765a",
            "--test",
            "read_track_fields",
            "--no-run",
            "--message-format=json",
        ],
        timeout=180,
        extra={
            "CARGO_TARGET_DIR": str(HERE / "target"),
            "CARGO_INCREMENTAL": "0",
            "CARGO_PROFILE_DEV_DEBUG": "0",
        },
        cwd=REPO,
    )
    executable = Path(
        next(
            a["executable"]
            for line in build.splitlines()
            if line.startswith(b"{") and (a := json.loads(line)).get("executable")
        )
    )
    record["emu_executable"] = {
        "path": str(executable),
        "sha256": sha(executable),
        "delivery": "source_build",
    }
    record["rustc"] = checked("rustc-version", ["rustc", "-Vv"]).decode()
    mame = required[0]
    record["mame_executable"] = {
        "path": str(mame),
        "sha256": sha(mame),
        "delivery": "installed_binary",
        "source_commit": "unknown",
        "version": checked("mame-version", [mame, "-help"]).decode().splitlines()[0],
    }
    record["mame_inspected_source"] = {
        "commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=MAME_SOURCE, text=True
        ).strip(),
        "sha256": sha(MAME_SOURCE / "src/devices/machine/upd765.cpp"),
        "executed_source_identity": False,
    }
    pages = [p.read_bytes() for p in required[1:]]
    record["firmware_sha256"] = [hashlib.sha256(p).hexdigest() for p in pages]
    encoder = load("mfi", RAW / "mfi.py")
    decoder = load("decode", RAW / "decode.py")
    for c in record["cases"]:
        (out / (c["name"] + ".txt")).write_text(input_text(c))
        if c["raw"]:
            raw = raw_track(c)
            image = encoder.encode(raw)
            (out / (c["name"] + ".raw")).write_bytes(raw)
            (out / (c["name"] + ".mfi")).write_bytes(image)
            c["mfi_audit"] = decoder.validate(image, raw)
    save()
    with tempfile.TemporaryDirectory(prefix="198x-read-track-fields-private-") as tmp:
        private = Path(tmp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0] + pages[1])
        (roms / "40093.ic8").write_bytes(pages[2] + pages[3])
        for repeat in (1, 2):
            for c in record["cases"]:
                for engine in ("emu", "fuse", "mame"):
                    name = f"{engine}-{repeat}-{c['name']}"
                    entry = {
                        "engine": engine,
                        "repeat": repeat,
                        "case": c["name"],
                        "name": name,
                        "evidence_class": "fresh_execution",
                    }
                    if engine != "emu" and not c["raw"]:
                        entry.update(
                            status="not_applicable",
                            reason="absent medium or out-of-model size guard; no comparable 6250-byte raw field fixture",
                        )
                        record["observations"].append(entry)
                        save()
                        continue
                    obsfile = private / "observation.json"
                    obsfile.unlink(missing_ok=True)
                    if engine == "emu":
                        args = [executable, "--nocapture", "--test-threads=1"]
                        env = {"READ_TRACK_FIELDS": str(out / (c["name"] + ".txt"))}
                        cwd = REPO
                    elif engine == "fuse":
                        args = [
                            binary,
                            out / (c["name"] + ".raw"),
                            *map(str, c["command"]),
                        ]
                        env = {}
                        cwd = HERE
                    else:
                        args = [
                            mame,
                            "specpls3",
                            "-bios",
                            "en",
                            "-rompath",
                            private,
                            "-flop1",
                            out / (c["name"] + ".mfi"),
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
                            HERE / "mame-probe.lua",
                            "-seconds_to_run",
                            "10",
                        ]
                        env = {
                            "SDL_VIDEODRIVER": "dummy",
                            "SDL_AUDIODRIVER": "dummy",
                            "PROBE_OUTPUT": str(obsfile),
                            "PROBE_COMMAND": " ".join(map(str, c["command"])),
                        }
                        cwd = private
                    media = out / (c["name"] + (".mfi" if engine == "mame" else ".raw"))
                    before = sha(media) if media.exists() else None
                    row, stdout, _stderr = run(
                        name, args, timeout=45, extra=env, cwd=cwd
                    )
                    entry["status"] = "fail"
                    if row["returncode"] == 0:
                        try:
                            if engine == "mame":
                                observed = json.loads(obsfile.read_text())
                            elif engine == "fuse":
                                observed = json.loads(stdout)
                            else:
                                line = next(
                                    line
                                    for line in stdout.decode().splitlines()
                                    if '{"data":' in line
                                )
                                observed = json.loads(line[line.index('{"data":') :])
                            if before and sha(media) != before:
                                raise ValueError("source media mutated")
                            observed["media_unchanged"] = True
                            entry.update(status="pass", observed=observed)
                        except (ValueError, StopIteration, OSError) as e:
                            entry["reason"] = str(e)
                    else:
                        entry["reason"] = (
                            "host timeout"
                            if row["timed_out"]
                            else "probe failed; see stderr"
                        )
                    record["observations"].append(entry)
                    save()
                    print(name, entry["status"], flush=True)
    record["private_copies_removed"] = not private.exists()
    record["status"] = "captured"
    save()
    files = {
        p.name: sha(p) for p in out.iterdir() if p.is_file() and p.name != "hashes.json"
    }
    (out / "hashes.json").write_text(json.dumps(files, indent=2) + "\n")


if __name__ == "__main__":
    main()

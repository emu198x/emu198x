#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Raw MAME controller study; prior Emu/Fuse evidence remains captured replay."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path

from decode import validate
from mfi import encode

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FAMILY = Path("/Users/stevehill/Projects/198x")
PRIOR = HERE.parent / "2026-10-19-read-track-drained-crc"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    trial = sys.argv[1:] == ["--trial"]
    require(not sys.argv[1:] or trial, "use --trial or no arguments")
    out = HERE / ("trial" if trial else "runs")
    out.mkdir(exist_ok=False)
    record = {
        "schema_version": 1,
        "qualified": False,
        "kind": "raw-mfi-controller-reference",
        "evidence_class": "fresh_reference_execution_with_captured_baseline",
        "host": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "fixtures": [],
        "observations": [],
        "trial": trial,
    }

    def save() -> None:
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")

    def execute(
        name: str,
        argv: list[str],
        timeout: int = 15,
        extra: dict[str, str] | None = None,
        cwd: Path = HERE,
    ) -> subprocess.CompletedProcess:
        env = {"LC_ALL": "C", "TZ": "UTC", **(extra or {})}
        start = time.monotonic()
        try:
            p = subprocess.run(
                argv,
                cwd=cwd,
                env={**os.environ, **env},
                capture_output=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            (out / (name + ".stdout")).write_bytes(error.stdout or b"")
            (out / (name + ".stderr")).write_bytes(error.stderr or b"")
            record["commands"].append(
                {
                    "name": name,
                    "argv": argv,
                    "cwd": str(cwd),
                    "environment": env,
                    "timed_out": True,
                    "host_timeout_seconds": timeout,
                    "host_seconds": time.monotonic() - start,
                }
            )
            save()
            raise
        (out / (name + ".stdout")).write_bytes(p.stdout)
        (out / (name + ".stderr")).write_bytes(p.stderr)
        record["commands"].append(
            {
                "name": name,
                "argv": argv,
                "cwd": str(cwd),
                "environment": env,
                "timed_out": False,
                "returncode": p.returncode,
                "host_timeout_seconds": timeout,
                "host_seconds": time.monotonic() - start,
            }
        )
        save()
        p.check_returncode()
        return p

    record["source"] = {
        "commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "authored": {
            p.name: sha(p)
            for p in [
                HERE / "run.py",
                HERE / "mfi.py",
                HERE / "decode.py",
                HERE / "mame-probe.lua",
            ]
        },
    }
    mame = Path(shutil.which("mame") or "").resolve()
    require(mame.is_file(), "missing prerequisite: MAME")
    record["mame"] = {
        "path": str(mame),
        "sha256": sha(mame),
        "source_commit": "unknown",
        "delivery": "installed_binary",
        "version": execute("mame-version", [str(mame), "-help"])
        .stdout.decode()
        .splitlines()[0],
    }
    formats = execute(
        "mame-media", [str(mame), "specpls3", "-listmedia"]
    ).stdout.decode()
    require(".mfi" in formats, "missing prerequisite: specpls3 raw MFI input")
    source = FAMILY / "emulators/multi-system/mame"
    record["inspected_mame_source"] = {
        "commit": subprocess.check_output(
            ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
        ).strip(),
        "executed_source_identity": False,
        "files": {
            p: sha(source / p)
            for p in [
                "src/lib/formats/mfi_dsk.cpp",
                "src/lib/formats/mfi_dsk.h",
                "src/lib/formats/flopimg.h",
                "src/devices/machine/upd765.cpp",
                "src/mame/sinclair/specpls3.cpp",
            ]
        },
    }
    record["baseline"] = {
        "kind": "captured_reference_replay",
        "archive": str(PRIOR / "evidence.tar.gz"),
        "sha256": sha(PRIOR / "evidence.tar.gz"),
        "emu_fuse_fresh_this_study": False,
    }
    spec = importlib.util.spec_from_file_location("prior_audit", PRIOR / "verify.py")
    require(spec is not None and spec.loader is not None, "missing prior auditor")
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    cases = [
        (order, kind, pos)
        for order in range(3)
        for kind, pos in [(0, 0), (1, 0), (2, 0), (1, 1), (2, 1)]
        if not (order == 0 and pos == 1)
    ]
    if trial:
        cases = cases[:3]
    pages = [
        (
            Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        ).read_bytes()
        for i in range(4)
    ]
    record["firmware_sha256"] = [hashlib.sha256(p).hexdigest() for p in pages]
    with tempfile.TemporaryDirectory(prefix="198x-raw-crc-baseline-") as old:
        oldroot = Path(old)
        with tarfile.open(PRIOR / "evidence.tar.gz") as archive:
            archive.extractall(oldroot, filter="data")
        record["baseline"]["audit"] = prior.audit(oldroot)
        baseline = json.loads((oldroot / "campaign.json").read_text())
        require(
            record["firmware_sha256"] == baseline["firmware_sha256"],
            "different local firmware",
        )
        indexed = {
            (x["engine"], x["repeat"], x["order"], x["kind"], x["position"]): x
            for x in baseline["observations"]
        }
        for order, kind, pos in cases:
            row = indexed[("fuse", 1, order, kind, pos)]
            raw = (oldroot / row["media"]).read_bytes()
            name = f"{order}-{kind}-{pos}"
            rawpath = out / (name + ".raw")
            rawpath.write_bytes(raw)
            media = out / (name + ".mfi")
            media.write_bytes(encode(raw))
            validation = validate(media.read_bytes(), raw)
            record["fixtures"].append(
                {
                    "order": order,
                    "kind": kind,
                    "position": pos,
                    "raw": rawpath.name,
                    "raw_sha256": sha(rawpath),
                    "media": media.name,
                    "media_sha256": sha(media),
                    "validation": validation,
                    "baseline_raw_sha256": row["media_sha256"],
                }
            )
        record["replayed_observations"] = [
            x for x in baseline["observations"] if x["engine"] in ("emu", "fuse")
        ]
    with tempfile.TemporaryDirectory(prefix="198x-raw-crc-private-") as tmp:
        private = Path(tmp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0] + pages[1])
        (roms / "40093.ic8").write_bytes(pages[2] + pages[3])
        for repeat in [1] if trial else [1, 2]:
            jobs = [("track", *case, 0x42) for case in cases]
            if not trial:
                jobs += [
                    (name, 0, kind, 0, op)
                    for name, op in [("id", 0x4A), ("data", 0x46)]
                    for kind in range(3)
                ]
            for name, order, kind, pos, op in jobs:
                count = 1 if order == 0 else 2
                expected = (
                    0 if name == "id" or (name == "data" and kind == 1) else count * 512
                )
                fixture = next(
                    x
                    for x in record["fixtures"]
                    if (x["order"], x["kind"], x["position"]) == (order, kind, pos)
                )
                media = out / fixture["media"]
                digest = sha(media)
                observed = private / "observed.json"
                observed.unlink(missing_ok=True)
                label = f"{name}-{repeat}-{order}-{kind}-{pos}"
                execute(
                    label,
                    [
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
                    ],
                    45,
                    {
                        "SDL_VIDEODRIVER": "dummy",
                        "SDL_AUDIODRIVER": "dummy",
                        "PROBE_OUTPUT": str(observed),
                        "PROBE_COUNT": str(count),
                        "PROBE_OPCODE": str(op),
                        "PROBE_BYTES": str(expected),
                    },
                    private,
                )
                require(sha(media) == digest, "source MFI changed")
                data = json.loads(observed.read_text())
                (out / (label + ".json")).write_bytes(observed.read_bytes())
                record["observations"].append(
                    {
                        "name": label,
                        "operation": name,
                        "repeat": repeat,
                        "order": order,
                        "kind": kind,
                        "position": pos,
                        "opcode": op,
                        "expected_payload_bytes": expected,
                        "media": fixture["media"],
                        "media_sha256": digest,
                        "media_unchanged": True,
                        "observed": data,
                    }
                )
                save()
                print(label, len(data["data"]), data["result"], flush=True)
    record["private_copies_removed"] = not private.exists()
    record["completed"] = True
    save()
    (out / "hashes.json").write_text(
        json.dumps(
            {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file()}, indent=2
        )
        + "\n"
    )
    print(
        json.dumps(
            {"fresh_mame_observations": len(record["observations"]), "qualified": False}
        )
    )


if __name__ == "__main__":
    main()

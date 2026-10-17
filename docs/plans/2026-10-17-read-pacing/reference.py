#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Capture installed-MAME read cadence/overrun; no ROM qualification claim."""

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
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(observed: dict, pause: int) -> None:
    if len(observed["result"]) != 7 or observed["idle_msr"] != 128:
        raise ValueError("incomplete result")
    if len(observed["times"]) != len(observed["data"]) or len(
        observed["after_read_msr"]
    ) != len(observed["data"]):
        raise ValueError("missing observations")
    if pause:
        if observed["result"][1] & 16 == 0:
            raise ValueError("missing overrun")
    else:
        if observed["data"] != [33] * 512 or observed["result"] != [
            64,
            128,
            0,
            0,
            0,
            1,
            2,
        ]:
            raise ValueError("payload or EOT differs")
        deltas = [b - a for a, b in zip(observed["times"][:8], observed["times"][1:8])]
        if len(deltas) != 7 or not all(
            30_000 <= round(d * 1_000_000_000) <= 34_000 for d in deltas
        ):
            raise ValueError("byte cadence differs")
        if not all(msr & 128 == 0 for msr in observed["after_read_msr"][:8]):
            raise ValueError("RQM does not clear")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--umbrella", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    mame = Path(shutil.which("mame") or "")
    if not mame.is_file():
        raise SystemExit("missing prerequisite: mame")
    recipe = json.loads(
        (args.umbrella / "ops/qualification/recipe-disk-plus3.json").read_text()
    )
    # Reuse the existing native diagnostic disk generator, not a new format writer.
    prior = HERE.parent / "2026-10-17-fdc-commands/reference/run.py"
    spec = importlib.util.spec_from_file_location("prior", prior)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    pages = [
        Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        for i in range(4)
    ]
    for page, wanted in zip(
        pages, [recipe["firmware"], *recipe["additional_firmware"]], strict=True
    ):
        if not page.is_file() or sha(page) != wanted["sha256"]:
            raise SystemExit("missing prerequisite or wrong firmware")
    record = {
        "schema_version": 1,
        "qualified": False,
        "kind": "direct-port-source-investigation",
        "origin": "fresh_execution",
        "host": platform.platform(),
        "python": platform.python_version(),
        "mame": {
            "path": str(mame),
            "sha256": sha(mame),
            "version": subprocess.check_output(
                [str(mame), "-help"], text=True, timeout=10
            ).splitlines()[0],
        },
        "firmware_sha256": [sha(p) for p in pages],
        "source_sha256": {
            p.name: sha(p) for p in [Path(__file__), HERE / "reference.lua", prior]
        },
        "cases": [],
    }
    with tempfile.TemporaryDirectory(prefix="198x-read-reference-") as temp:
        private = Path(temp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0].read_bytes() + pages[1].read_bytes())
        (roms / "40093.ic8").write_bytes(pages[2].read_bytes() + pages[3].read_bytes())
        for repeat in [1, 2]:
            for opcode in [0x46, 0x4C, 0x42]:
                for pause in [0, 1000]:
                    name = f"{repeat}-{opcode:02x}-{pause}"
                    disk = out / (name + ".dsk")
                    disk.write_bytes(module.disk([1], deleted=opcode == 0x4C))
                    observed = out / (name + ".json")
                    env = {
                        "SDL_VIDEODRIVER": "dummy",
                        "SDL_AUDIODRIVER": "dummy",
                        "PROBE_OUTPUT": str(observed),
                        "PROBE_OPCODE": str(opcode),
                        "PROBE_PAUSE": str(pause),
                    }
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
                        "-autoboot_delay",
                        "0",
                        "-autoboot_script",
                        str(HERE / "reference.lua"),
                        "-seconds_to_run",
                        "10",
                    ]
                    before = sha(disk)
                    p = subprocess.run(
                        argv,
                        cwd=private,
                        env={**os.environ, **env},
                        capture_output=True,
                        timeout=45,
                        check=False,
                    )
                    (out / (name + ".stdout")).write_bytes(p.stdout)
                    (out / (name + ".stderr")).write_bytes(p.stderr)
                    case = {
                        "repeat": repeat,
                        "opcode": opcode,
                        "pause_us": pause,
                        "argv": argv,
                        "environment": env,
                        "timeout_seconds": 45,
                        "exit": p.returncode,
                        "media_sha256": before,
                        "media_unchanged": sha(disk) == before,
                        "observed": json.loads(observed.read_text())
                        if observed.exists()
                        else None,
                    }
                    record["cases"].append(case)
                    (out / "result.json").write_text(
                        json.dumps(record, indent=2) + "\n"
                    )
                    p.check_returncode()
                    if not case["media_unchanged"]:
                        raise ValueError("media modified")
                    check(case["observed"], pause)
    record["checks_passed"] = True
    (out / "result.json").write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    main()

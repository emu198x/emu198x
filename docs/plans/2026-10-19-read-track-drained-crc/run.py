#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Capture bounded source-controller comparisons. Never a ROM qualification."""

from __future__ import annotations

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
REPO = HERE.parents[2]
FAMILY = Path("/Users/stevehill/Projects/198x")
FUSE = FAMILY / "emulators/zx-spectrum/fuse-emulator-fuse"
MAME_SOURCE = FAMILY / "emulators/multi-system/mame"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    out = HERE / "runs"
    out.mkdir(exist_ok=False)
    record = {
        "schema_version": 1,
        "qualified": False,
        "evidence_class": "fresh_execution",
        "kind": "fully-drained-controller-comparison",
        "host": platform.platform(),
        "python": platform.python_version(),
        "commands": [],
        "observations": [],
    }

    def save() -> None:
        (out / "campaign.json").write_text(json.dumps(record, indent=2) + "\n")

    def run(
        name: str,
        argv: list[str],
        timeout: int = 15,
        extra: dict[str, str] | None = None,
        cwd: Path = HERE,
    ) -> subprocess.CompletedProcess:
        begin = time.monotonic()
        env = {"LC_ALL": "C", "TZ": "UTC", **(extra or {})}
        try:
            process = subprocess.run(
                argv,
                cwd=cwd,
                env={**os.environ, **env},
                capture_output=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            (out / f"{name}.stdout").write_bytes(error.stdout or b"")
            (out / f"{name}.stderr").write_bytes(error.stderr or b"")
            record["commands"].append(
                {
                    "name": name,
                    "argv": argv,
                    "cwd": str(cwd),
                    "environment": env,
                    "timeout_seconds": timeout,
                    "timed_out": True,
                    "host_seconds": time.monotonic() - begin,
                }
            )
            save()
            raise
        (out / f"{name}.stdout").write_bytes(process.stdout)
        (out / f"{name}.stderr").write_bytes(process.stderr)
        record["commands"].append(
            {
                "name": name,
                "argv": argv,
                "cwd": str(cwd),
                "environment": env,
                "timeout_seconds": timeout,
                "timed_out": False,
                "returncode": process.returncode,
                "host_seconds": time.monotonic() - begin,
            }
        )
        save()
        process.check_returncode()
        return process

    record["emu_source"] = {
        "commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        "controller_sha256": sha(REPO / "crates/nec-upd765a/src/lib.rs"),
        "test_sha256": sha(REPO / "crates/nec-upd765a/tests/read_track_drained.rs"),
    }
    record["authored_sources"] = {
        str(p.relative_to(REPO)): sha(p)
        for p in [
            HERE / "run.py",
            HERE / "host.c",
            HERE / "fuse_probe.c",
            HERE / "mame-probe.lua",
            *sorted((HERE / "host").glob("*")),
        ]
        if p.is_file()
    }
    fuse_commit = subprocess.check_output(
        ["git", "-C", str(FUSE), "rev-parse", "HEAD"], text=True
    ).strip()
    if fuse_commit != "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2":
        raise ValueError("different reference source")
    tracked_status = subprocess.check_output(
        ["git", "-C", str(FUSE), "status", "--porcelain", "--untracked-files=no"],
        text=True,
    )
    if tracked_status.strip():
        raise ValueError("reference source has tracked changes")
    record["fuse_source"] = {
        "commit": fuse_commit,
        "patched": False,
        "files": {
            p: sha(FUSE / p)
            for p in [
                "event.h",
                "bitmap.h",
                *[
                    "peripherals/disk/" + n
                    for n in [
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
        },
    }
    clang = Path(shutil.which("clang") or "").resolve()
    record["compiler"] = {
        "path": str(clang),
        "sha256": sha(clang),
        "version": run("clang-version", [str(clang), "--version"]).stdout.decode(),
    }
    binary = HERE / "fuse-stock"
    run(
        "fuse-build",
        [
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
            *[
                str(FUSE / "peripherals/disk" / n)
                for n in ["upd_fdc.c", "fdd.c", "crc.c"]
            ],
            "-o",
            str(binary),
        ],
        60,
    )
    record["fuse_executable"] = {
        "path": str(binary),
        "sha256": sha(binary),
        "delivery": "source_build",
    }
    cases = [
        (order, kind, position)
        for order in range(3)
        for kind, position in [(0, 0), (1, 0), (2, 0), (1, 1), (2, 1)]
        if not (order == 0 and position == 1)
    ]
    for repeat in (1, 2):
        for order, kind, position in cases:
            name = f"fuse-{repeat}-{order}-{kind}-{position}"
            raw = out / (name + ".raw")
            process = run(
                name,
                [
                    str(binary),
                    str(order),
                    str(kind),
                    str(position),
                    str(repeat),
                    str(raw),
                ],
            )
            assert not process.stderr, "reference/sanitizer diagnostic"
            record["observations"].append(
                {
                    "engine": "fuse",
                    "name": name,
                    "order": order,
                    "repeat": repeat,
                    "kind": kind,
                    "position": position,
                    "media": raw.name,
                    "media_sha256": sha(raw),
                    "observed": json.loads(process.stdout),
                }
            )
            save()

    build = run(
        "emu-build",
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "nec-upd765a",
            "--test",
            "read_track_drained",
            "--no-run",
            "--message-format=json",
        ],
        180,
        {"CARGO_TARGET_DIR": str(HERE / "target"), "CARGO_INCREMENTAL": "0"},
        REPO,
    )
    artifacts = [
        json.loads(line) for line in build.stdout.splitlines() if line.startswith(b"{")
    ]
    executable = Path(next(a["executable"] for a in artifacts if a.get("executable")))
    record["emu_executable"] = {
        "path": str(executable),
        "sha256": sha(executable),
        "delivery": "source_build",
    }
    record["rustc"] = run("rustc-version", ["rustc", "-Vv"]).stdout.decode()
    emu = run(
        "emu-observations", [str(executable), "--nocapture", "--test-threads=1"], 30
    )
    for line in emu.stdout.decode().splitlines():
        if '{"repeat":' not in line:
            continue
        obs = json.loads(line[line.index('{"repeat":') :])
        order = 0 if len(obs["ids"]) == 1 else (1 if obs["ids"][0] == 1 else 2)
        record["observations"].append(
            {
                "engine": "emu",
                "name": f"emu-{obs['repeat']}-{order}-{obs['kind']}-{obs['position']}",
                "order": order,
                "repeat": obs["repeat"],
                "kind": obs["kind"],
                "position": obs["position"],
                "observed": obs,
            }
        )
    save()

    mame = Path(shutil.which("mame") or "").resolve()
    record["mame_executable"] = {
        "path": str(mame),
        "sha256": sha(mame),
        "delivery": "installed_binary",
        "source_commit": "unknown",
        "version": run("mame-version", [str(mame), "-help"])
        .stdout.decode()
        .splitlines()[0],
    }
    record["mame_inspected_source"] = {
        "commit": subprocess.check_output(
            ["git", "-C", str(MAME_SOURCE), "rev-parse", "HEAD"], text=True
        ).strip(),
        "path": "src/devices/machine/upd765.cpp",
        "sha256": sha(MAME_SOURCE / "src/devices/machine/upd765.cpp"),
        "executed_source_identity": False,
    }
    spec = importlib.util.spec_from_file_location(
        "prior_disk", REPO / "docs/plans/2026-10-17-fdc-commands/reference/run.py"
    )
    assert spec and spec.loader
    prior = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prior)
    pages = [
        (
            Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        ).read_bytes()
        for i in range(4)
    ]
    record["firmware_sha256"] = [hashlib.sha256(p).hexdigest() for p in pages]
    with tempfile.TemporaryDirectory(prefix="198x-drained-crc-private-") as tmp:
        private = Path(tmp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0] + pages[1])
        (roms / "40093.ic8").write_bytes(pages[2] + pages[3])
        for repeat in (1, 2):
            for order, kind, position in cases:
                ids = [[1], [1, 2], [2, 1]][order]
                name = f"mame-{repeat}-{order}-{kind}-{position}"
                media = out / (name + ".dsk")
                data = bytearray(prior.disk(ids))
                if kind:
                    data[280 + 8 * position + 4] = 0x20
                if kind == 2:
                    data[280 + 8 * position + 5] = 0x20
                media.write_bytes(data)
                digest = sha(media)
                observation = private / "observation.json"
                observation.unlink(missing_ok=True)
                run(
                    name,
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
                        "PROBE_OUTPUT": str(observation),
                        "PROBE_COUNT": str(len(ids)),
                    },
                    private,
                )
                assert sha(media) == digest, "source medium changed"
                obs = json.loads(observation.read_text())
                obs["media_unchanged"] = True
                (out / (name + ".json")).write_text(json.dumps(obs, indent=2) + "\n")
                record["observations"].append(
                    {
                        "engine": "mame",
                        "name": name,
                        "order": order,
                        "repeat": repeat,
                        "kind": kind,
                        "position": position,
                        "media": media.name,
                        "media_sha256": digest,
                        "observed": obs,
                    }
                )
                save()
    record["completed"] = True
    save()
    (out / "hashes.json").write_text(
        json.dumps(
            {p.name: sha(p) for p in sorted(out.iterdir()) if p.is_file()}, indent=2
        )
        + "\n"
    )
    print(json.dumps({"observations": len(record["observations"]), "qualified": False}))


if __name__ == "__main__":
    main()

# SPDX-License-Identifier: GPL-2.0-or-later
"""Fresh, bounded MAME controller observations; no ROM qualification claim."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def disk(ids: list[int], deleted: bool = False) -> bytes:
    # Standard DSK recipe used by the umbrella cylinder/EOT probes.
    size = 256 + 512 * len(ids)
    data = bytearray(256 + size)
    data[:8] = b"MV - CPC"
    data[48:50] = bytes([1, 1])
    data[50:52] = size.to_bytes(2, "little")
    data[256:268] = b"Track-Info\r\n"
    data[276:280] = bytes([2, len(ids), 0x2A, 0xE5])
    for i, r in enumerate(ids):
        off = 280 + 8 * i
        data[off : off + 8] = bytes([0, 0, r, 2, 0, 0x40 if deleted else 0, 0, 2])
        data[512 + i * 512 : 1024 + i * 512] = bytes([0x20 + r]) * 512
    return bytes(data)


def main() -> None:
    out = Path(sys.argv[1])
    if out.exists():
        raise SystemExit("output exists")
    mame = shutil.which("mame")
    if not mame:
        raise SystemExit("missing prerequisite: mame")
    pages = [
        (
            Path.home() / f".emu198x/roms/amstrad-zx-spectrum-plus3/plus3-{i}.rom"
        ).read_bytes()
        for i in range(4)
    ]
    record = {
        "kind": "direct-controller-reference-not-rom-qualification",
        "evidence_class": "fresh_execution",
        "mame": {
            "path": mame,
            "sha256": sha(Path(mame).read_bytes()),
            "version": subprocess.check_output([mame, "-help"], text=True).splitlines()[
                0
            ],
        },
        "firmware_sha256": [sha(p) for p in pages],
        "cases": [],
    }
    cases = [("write-deleted", 0x49, 0xA5, [1], 1, 1, 1, False)]
    for op in [0x51, 0x59, 0x5D]:
        for name, host in [
            ("equal", 0x21),
            ("disk-less", 0x22),
            ("disk-greater", 0x20),
        ]:
            cases.append((f"{op:02x}-{name}", op, host, [1], 1, 1, 1, False))
    cases += [
        ("scan-early-hit", 0x51, 0x21, [1, 2], 1, 2, 1, False),
        ("scan-mixed-low", 0x59, 0x21, [1], 1, 1, 1, False),
        ("scan-step", 0x51, 0x23, [1, 2, 3], 1, 3, 2, False),
        ("scan-deleted", 0x51, 0x21, [1], 1, 1, 1, True),
        ("track-order", 0x42, 0, [3, 1, 2], 1, 3, 0xFF, False),
        ("track-count", 0x42, 0, [3, 1, 2], 1, 2, 0xFF, False),
    ]
    with tempfile.TemporaryDirectory(prefix="198x-fdc-command-reference-") as tmp:
        private = Path(tmp)
        roms = private / "specpls3"
        roms.mkdir()
        (roms / "40092.ic7").write_bytes(pages[0] + pages[1])
        (roms / "40093.ic8").write_bytes(pages[2] + pages[3])
        for name, op, host, ids, r, eot, stp, deleted in cases:
            source = disk(ids, deleted)
            if name == "scan-mixed-low":
                mixed = bytearray(source)
                mixed[512:1024] = bytes([0x22]) + bytes([0x20]) * 511
                source = bytes(mixed)
            media = private / "probe.dsk"
            media.write_bytes(source)
            obs = private / "observed.json"
            obs.unlink(missing_ok=True)
            env = {
                "SDL_VIDEODRIVER": "dummy",
                "SDL_AUDIODRIVER": "dummy",
                "PROBE_OUTPUT": str(obs),
                "PROBE_OPCODE": str(op),
                "PROBE_FILL": str(host),
                "PROBE_R": str(r),
                "PROBE_EOT": str(eot),
                "PROBE_STP": str(stp),
            }
            argv = [
                mame,
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
                str(HERE / "probe.lua"),
                "-seconds_to_run",
                "10",
            ]
            p = subprocess.run(
                argv,
                env={**os.environ, **env},
                cwd=private,
                capture_output=True,
                text=True,
                timeout=45,
                check=False,
            )
            row = {
                "name": name,
                "opcode": op,
                "host": host,
                "ids": ids,
                "first": r,
                "eot": eot,
                "stp": stp,
                "deleted": deleted,
                "media_sha256": sha(source),
                "command": argv,
                "environment": env,
                "exit": p.returncode,
                "stdout": p.stdout,
                "stderr": p.stderr,
            }
            if obs.exists():
                row["observation"] = json.loads(obs.read_text())
            record["cases"].append(row)
            out.write_text(json.dumps(record, indent=2) + "\n")
            print(
                name,
                p.returncode,
                row.get("observation", {}).get("observed", {}).get("result"),
                flush=True,
            )
            if p.returncode or "observation" not in row:
                raise SystemExit("reference failed; inspect retained logs")
    record["sources"] = {
        p.name: sha(p.read_bytes()) for p in [Path(__file__), HERE / "probe.lua"]
    }
    out.write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    main()

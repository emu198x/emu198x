"""Bounded original-cartridge reproduction of #2136; exits 1 on extra frames."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    cart = args.output / "frame-probe.sfc"
    data = bytearray(32768)
    data[:3] = bytes([0x4C, 0, 0x80])  # JMP $8000
    data[0x7FC0:0x7FD5] = b"EMU198X FRAME PROBE".ljust(21, b" ")
    data[0x7FD5] = 0x20
    data[0x7FD7] = 5
    data[0x7FD9] = 1
    data[0x7FFA:0x7FFE] = bytes([0, 0x80, 0, 0x80])
    data[0x7FDC:0x7FE0] = bytes([255, 255, 0, 0])
    checksum = sum(data) & 65535
    data[0x7FDC:0x7FE0] = ((~checksum) & 65535).to_bytes(
        2, "little"
    ) + checksum.to_bytes(2, "little")
    cart.write_bytes(data)
    results: list[dict[str, str | int]] = []
    for model in ["snes-ntsc", "snes-pal"]:
        for requested in [1, 2, 5]:
            command = [
                str(binary),
                "--headless",
                "--model",
                model,
                "--cart",
                str(cart),
                "--frames",
                str(requested),
            ]
            run = subprocess.run(
                command, capture_output=True, text=True, timeout=60, check=True
            )
            report = json.loads(run.stdout)
            row = {
                "model": model,
                "requested": requested,
                "reported": report["frames_run"],
                "time": report["time"],
            }
            results.append(row)
            print(json.dumps(row), flush=True)
    assert cart.read_bytes() == data, "loaded input changed"
    (args.output / "observations.json").write_text(json.dumps(results, indent=2) + "\n")
    return int(any(row["reported"] != row["requested"] for row in results))


if __name__ == "__main__":
    raise SystemExit(main())

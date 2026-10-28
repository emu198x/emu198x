"""Run the original GxROM guest through a compiled Mesen core in fresh homes."""

import argparse
import hashlib
import json
import subprocess
import zlib
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    rom = args.rom.resolve(strict=True)
    args.output.mkdir(parents=True)
    crc = f"{zlib.crc32(rom.read_bytes()[16:]):08X}"
    expected = bytearray(256)
    for prg in range(4):
        for chr_bank in range(4):
            expected[prg * 4 + chr_bank] = 0x80 + prg
            expected[0x40 + prg * 4 + chr_bank] = 0xA0 + chr_bank
    expected[0x20:0x22] = bytes([0x81, 0x82])
    expected[0x60:0x62] = bytes([0xA1, 0xA2])
    expected[0xFF] = 0xA5
    receipts = []
    for name, conflict in [("default", None), ("and", "Y"), ("unmasked", "N")]:
        home = args.output / name
        home.mkdir()
        if conflict:
            row = [
                crc,
                "NesNtsc",
                "",
                "",
                "",
                "66",
                "128",
                "32",
                "0",
                "0",
                "0",
                "0",
                "H",
                "0",
                conflict,
                "0",
                "0",
                "0",
            ]
            (home / "MesenNesDB.txt").write_text(",".join(row) + "\n")
        result = subprocess.run(
            [str(binary), str(home.resolve()), str(rom)],
            check=True,
            capture_output=True,
            text=True,
            timeout=15,
        )
        (home / "stdout.txt").write_text(result.stdout)
        (home / "stderr.txt").write_text(result.stderr)
        lines = [
            line
            for line in result.stdout.splitlines()
            if len(line) == 512 and all(c in "0123456789abcdef" for c in line)
        ]
        assert len(lines) == 1, result.stdout
        observed = bytes.fromhex(lines[0])
        differing = [
            i for i, (a, b) in enumerate(zip(observed, expected, strict=True)) if a != b
        ]
        if conflict == "Y":
            assert not differing, differing
        else:
            assert differing == [0x20, 0x21, 0x60, 0x61], differing
            assert [observed[i] for i in differing] == [0x83, 0x83, 0xA3, 0xA3]
        receipts.append(
            {
                "mode": name,
                "differing_ram_offsets": differing,
                "conflict_observations": [
                    observed[i] for i in [0x20, 0x21, 0x60, 0x61]
                ],
            }
        )
    assert len(receipts) == 3
    print(
        json.dumps(
            {
                "rom_sha256": hashlib.sha256(rom.read_bytes()).hexdigest(),
                "body_crc32": crc,
                "receipts": receipts,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()

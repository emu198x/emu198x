#!/usr/bin/env python3
"""Write new copies of the project-owned 68EC020 fixture program."""

import argparse
import pathlib
import struct


def firmware(active: bool, interlace: bool) -> bytes:
    rom = bytearray(512 * 1024)
    rom[:8] = struct.pack(">II", 0x001FF000, 0x00F80008)
    if not active:
        rom[8:10] = bytes.fromhex("60fe")
        return bytes(rom)
    code = bytearray()
    for value, address in [(3, 0xBFE201), (2, 0xBFE001)]:
        code.extend(struct.pack(">HHI", 0x13FC, value, address))
    for address, value in [
        (0x1000, 0x7F80),
        (0x1002, 0x4080),
        (0xDFF180, 0x0F00),
        (0xDFF100, 4 if interlace else 0),
        (0xDFF0A0, 0),
        (0xDFF0A2, 0x1000),
        (0xDFF0A4, 2),
        (0xDFF0A6, 200),
        (0xDFF0A8, 64),
        (0xDFF096, 0x8201),
    ]:
        code.extend(struct.pack(">HHI", 0x33FC, value, address))
    loop = len(code)
    code.extend(struct.pack(">HI", 0x5279, 0x1200))
    code.extend(struct.pack(">HII", 0x33F9, 0xDFF00C, 0x1202))
    code.extend(struct.pack(">HII", 0x13F9, 0xBFE001, 0x1204))
    code.extend(struct.pack(">Bb", 0x60, loop - len(code) - 2))
    rom[8 : 8 + len(code)] = code
    return bytes(rom)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=pathlib.Path)
    root = parser.parse_args().output
    root.mkdir(parents=True, exist_ok=True)
    for name, active, interlace in [
        ("active-progressive", True, False),
        ("active-interlaced", True, True),
        ("inactive", False, False),
    ]:
        with (root / f"{name}.rom").open("xb") as output:
            output.write(firmware(active, interlace))


if __name__ == "__main__":
    main()

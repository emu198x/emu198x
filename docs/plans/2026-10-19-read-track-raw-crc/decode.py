# SPDX-License-Identifier: GPL-2.0-or-later
"""Independent MFI cell/clock validator; does not use fixture encoder logic."""

from __future__ import annotations

import struct
import zlib


def validate(image: bytes, raw: bytes) -> dict:
    def require(ok: bool, reason: str) -> None:
        if not ok:
            raise ValueError(reason)

    require(len(image) > 48, "truncated MFI")
    require(
        struct.unpack("<16sIIII", image[:32])
        == (b"MAMEFLOPPYIMAGE\0", 1, 1, 0x20202033, 0x44445353),
        "wrong geometry or MFI revision",
    )
    offset, compressed, size, splice = struct.unpack("<IIII", image[32:48])
    require(
        offset == 48
        and compressed == len(image) - 48
        and splice == 0
        and size > 0
        and size % 4 == 0,
        "wrong track directory",
    )
    require(size <= 400000 and compressed <= 500000, "oversized MFI track")
    inflater = zlib.decompressobj()
    stream = inflater.decompress(image[48:], 400001)
    require(
        inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail,
        "unbounded or trailing compressed track",
    )
    require(len(stream) == size, "wrong decompressed track length")
    cells = [0] * 100000
    position = 0
    for (value,) in struct.iter_unpack("<I", stream):
        require(value >> 28 == 0 and value > 0, "non-flux or duplicate transition")
        position += value
        require(
            position < 200000000 and (position + 500) % 2000 == 0,
            "off-grid flux transition",
        )
        index = (position + 500) // 2000 - 1
        require(not cells[index], "duplicate cell")
        cells[index] = 1
    recovered = bytearray()
    marked = []
    previous = cells[-1]
    # Independently inspect each clock/data pair, recognizing A1's missing clock.
    for offset in range(0, len(cells), 16):
        group = cells[offset : offset + 16]
        word = int("".join(map(str, group)), 2)
        data = int("".join(map(str, group[1::2])), 2)
        if word == 0x4489:
            require(data == 0xA1, "wrong sync value")
            marked.append(offset // 16)
        else:
            for clock, bit in zip(group[::2], group[1::2], strict=True):
                require(clock == (0 if previous or bit else 1), "invalid MFM clock")
                previous = bit
        previous = group[-1]
        recovered.append(data)
    require(bytes(recovered) == raw[3:6253], "raw bytes changed in MFI")
    expected = [i for i in range(6250) if raw[6253 + i // 8] & (1 << (i % 8))]
    require(marked == expected, "missing-clock marks changed in MFI")
    return {
        "cells": len(cells),
        "raw_bytes": len(recovered),
        "sync_marks": len(marked),
        "transitions": len(stream) // 4,
    }

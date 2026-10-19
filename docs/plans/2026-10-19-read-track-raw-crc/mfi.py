# SPDX-License-Identifier: GPL-2.0-or-later
"""Bounded authored MFI fixture, not a production format/mastering interface.

MAME mfi_dsk.{cpp,h} define the container; existing Fuse raw fixtures provide
independently audited byte/clock/CRC content. Only C0/H0 is represented.
"""

from __future__ import annotations

import struct
import zlib


def encode(raw: bytes) -> bytes:
    if len(raw) != 8599 or raw[:3] != bytes([106, 24, 0]):
        raise ValueError("expected the audited 6250-byte Fuse raw track")
    track, clocks = raw[3:6253], raw[6253:7035]
    transitions = []
    previous = 0
    cell = 0
    last = 0
    for index, byte in enumerate(track):
        if clocks[index // 8] & (1 << (index % 8)):
            if byte != 0xA1:
                raise ValueError("unsupported missing-clock byte")
            word = 0x4489
        else:
            word = 0
            for shift in range(7, -1, -1):
                bit = (byte >> shift) & 1
                word = (word << 2) | ((not (previous or bit)) << 1) | bit
                previous = bit
        previous = byte & 1
        for shift in range(15, -1, -1):
            cell += 1
            if word & (1 << shift):
                position = cell * 2000 - 500
                transitions.append(position - last)
                last = position
    payload = struct.pack("<" + "I" * len(transitions), *transitions)
    compressed = zlib.compress(payload)
    header = struct.pack("<16sIIII", b"MAMEFLOPPYIMAGE\0", 1, 1, 0x20202033, 0x44445353)
    entry = struct.pack("<IIII", 48, len(compressed), len(payload), 0)
    return header + entry + compressed

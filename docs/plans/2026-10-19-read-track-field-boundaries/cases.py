# SPDX-License-Identifier: GPL-2.0-or-later
"""Finite authored matrix. Raw fields and sector representations are distinct."""

from __future__ import annotations

import binascii


def matrix() -> list[dict]:
    cases = []

    def add(
        name, n=2, dtl=255, sectors=None, eot=1, present=True, raw=True, safety=False
    ):
        if sectors is None:
            sectors = [[0, 0, 1, n, 128 << n, 33]]
        cases.append(
            {
                "name": name,
                "command": [66, 0, 0, 0, 1, n, eot, 42, dtl],
                "sectors": sectors,
                "present": present,
                "raw": raw,
                "safety_only": safety,
            }
        )

    for n in range(6):
        add(f"n{n}", n)
    for dtl in (0, 1, 127, 128):
        add(f"n0-dtl-{dtl}", 0, dtl)
    add("n2-dtl-1", dtl=1)
    for cn, rn in ((0, 2), (2, 0), (1, 2), (3, 2)):
        add(f"command-n{cn}-recorded-n{rn}", cn, sectors=[[0, 0, 1, rn, 128 << rn, 33]])
    # Raw deliberately puts a valid CRC after the ACTUAL field length. It does
    # not pretend the zero padding of a sector image exists on the medium.
    for length in (0, 128, 512, 768):
        add(f"represented-{length}", sectors=[[0, 0, 1, 2, length, 33]])
    add("empty-track", sectors=[])
    add("absent-media", present=False, raw=False)
    for field in range(4):
        s = [0, 0, 1, 2, 512, 33]
        s[field] += 1
        add(f"id-mismatch-{'chrn'[field]}", sectors=[s])
    for order in ((1, 2), (2, 1), (1, 1)):
        for eot in (1, 2, 3):
            add(
                "ids-" + "".join(map(str, order)) + f"-eot-{eot}",
                sectors=[[0, 0, r, 2, 512, 33 + i] for i, r in enumerate(order)],
                eot=eot,
            )
    add("eot-0", eot=0)
    for n in (6, 7, 8, 255):
        add(
            f"safety-n{n}", n=n, sectors=[[0, 0, 1, 2, 512, 33]], raw=False, safety=True
        )
    return cases


def raw_track(case: dict) -> bytes:
    data = bytearray()
    clocks = bytearray(782)

    def put(byte: int, clock: bool = False) -> None:
        if len(data) >= 6250:
            raise ValueError("track does not fit 250kbit/s revolution")
        if clock:
            clocks[len(data) // 8] |= 1 << (len(data) % 8)
        data.append(byte)

    def gap(n: int, byte: int) -> None:
        for _ in range(n):
            put(byte)

    def field(body: bytes | list[int]) -> None:
        message = b"\xa1" * 3 + bytes(body)
        for b in message[:3]:
            put(b, True)
        for b in message[3:]:
            put(b)
        crc = binascii.crc_hqx(message, 65535)
        put(crc >> 8)
        put(crc & 255)

    gap(80, 78)
    for c, h, r, n, length, value in case["sectors"]:
        gap(12, 0)
        field([254, c, h, r, n])
        gap(22, 78)
        gap(12, 0)
        field(bytes([251]) + bytes([value]) * length)
        gap(42, 78)
    gap(6250 - len(data), 78)
    return bytes([106, 24, 0]) + data + clocks + bytes(1564)


def input_text(case: dict) -> str:
    return (
        "\n".join(
            [
                case["name"],
                " ".join(map(str, case["command"])),
                str(int(case["present"])),
                *[" ".join(map(str, s)) for s in case["sectors"]],
            ]
        )
        + "\n"
    )

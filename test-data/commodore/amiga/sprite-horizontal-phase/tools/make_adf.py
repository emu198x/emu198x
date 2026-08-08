#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Create and validate a deterministic bootable Amiga disk image."""

from __future__ import annotations

import argparse
import hashlib
import struct
from pathlib import Path

ADF_BYTES = 80 * 2 * 11 * 512
BOOTBLOCK_BYTES = 1024
SECTOR_BYTES = 512
CHECKSUM_OFFSET = 4
PAYLOAD_OFFSET = BOOTBLOCK_BYTES


class AdfError(ValueError):
    """Raised when an input cannot form the corpus disk layout."""


def _end_around_sum(words: bytes) -> int:
    """Return the 32-bit one's-complement end-around-carry sum."""

    if len(words) % 4 != 0:
        raise AdfError("checksum input length must be divisible by four")

    total = 0
    for (word,) in struct.iter_unpack(">I", words):
        total += word
        total = (total & 0xFFFFFFFF) + (total >> 32)
    return total & 0xFFFFFFFF


def bootblock_checksum(block: bytes) -> int:
    """Calculate the checksum word for a zeroed 1,024-byte boot block."""

    if len(block) != BOOTBLOCK_BYTES:
        raise AdfError(f"boot block must be {BOOTBLOCK_BYTES} bytes")

    candidate = bytearray(block)
    candidate[CHECKSUM_OFFSET : CHECKSUM_OFFSET + 4] = b"\0\0\0\0"
    return (~_end_around_sum(candidate)) & 0xFFFFFFFF


def pack_adf(boot: bytes, payload: bytes) -> tuple[bytes, int, int]:
    """Pack boot and payload bytes, returning image, checksum, and sectors."""

    if not boot.startswith(b"DOS\0"):
        raise AdfError("boot block must begin with the DOS\\0 signature")
    if len(boot) > BOOTBLOCK_BYTES:
        raise AdfError(
            f"boot block is {len(boot)} bytes; maximum is {BOOTBLOCK_BYTES}"
        )
    if not payload:
        raise AdfError("payload must not be empty")
    if PAYLOAD_OFFSET + len(payload) > ADF_BYTES:
        raise AdfError("payload does not fit in an 880 KiB disk image")

    bootblock = bytearray(BOOTBLOCK_BYTES)
    bootblock[: len(boot)] = boot
    bootblock[CHECKSUM_OFFSET : CHECKSUM_OFFSET + 4] = b"\0\0\0\0"
    checksum = bootblock_checksum(bootblock)
    bootblock[CHECKSUM_OFFSET : CHECKSUM_OFFSET + 4] = struct.pack(">I", checksum)

    image = bytearray(ADF_BYTES)
    image[:BOOTBLOCK_BYTES] = bootblock
    image[PAYLOAD_OFFSET : PAYLOAD_OFFSET + len(payload)] = payload
    sectors = (len(payload) + SECTOR_BYTES - 1) // SECTOR_BYTES

    validate_adf(bytes(image), payload, sectors)
    return bytes(image), checksum, sectors


def validate_adf(image: bytes, payload: bytes, sectors: int) -> None:
    """Validate the fixed disk layout and boot checksum."""

    if len(image) != ADF_BYTES:
        raise AdfError(f"ADF must be exactly {ADF_BYTES} bytes")
    if image[:4] != b"DOS\0":
        raise AdfError("ADF does not contain a DOS\\0 boot signature")
    if _end_around_sum(image[:BOOTBLOCK_BYTES]) != 0xFFFFFFFF:
        raise AdfError("boot-block checksum does not sum to 0xffffffff")
    if image[PAYLOAD_OFFSET : PAYLOAD_OFFSET + len(payload)] != payload:
        raise AdfError("payload bytes do not round-trip through the ADF")

    padded_end = PAYLOAD_OFFSET + sectors * SECTOR_BYTES
    padding = image[PAYLOAD_OFFSET + len(payload) : padded_end]
    if any(padding):
        raise AdfError("payload sector padding must be zero")
    if any(image[padded_end:]):
        raise AdfError("unused disk bytes must be zero")


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Pack one corpus boot block and payload into a bootable ADF."
    )
    parser.add_argument("boot", type=Path)
    parser.add_argument("payload", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    boot = args.boot.read_bytes()
    payload = args.payload.read_bytes()
    image, checksum, sectors = pack_adf(boot, payload)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(image)
    print(
        f"{args.output}: {len(image)} bytes, payload {len(payload)} bytes/"
        f"{sectors} sectors, boot checksum 0x{checksum:08x}, "
        f"sha256 {_sha256(image)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

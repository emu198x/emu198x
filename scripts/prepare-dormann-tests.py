#!/usr/bin/env python3
"""Stage pinned Dormann/Clark programme fixtures without an assembler dependency.

The interrupt source is GPL-3.0-or-later; Clark's decimal source states public
domain. Download their source and listings alongside the binaries. No payloads
belong in this repository. See test-data/6502-dormann-programmes.md.
"""

import argparse
import gzip
import hashlib
from pathlib import Path
from urllib.request import urlopen

INTERRUPT_BASE = (
    "https://raw.githubusercontent.com/freewilll/apple2-go/"
    "5899396fb6d578289eb67b7a83333282cade04c3/cpu/"
)
DECIMAL_BASE = (
    "https://raw.githubusercontent.com/JetSetIlly/Gopher2600/"
    "823f26b152140feccc3be79d9719140c6797e4db/"
    "hardware/cpu/tests/klaus2m5/decimal_mode/"
)
SOURCES = [
    (
        INTERRUPT_BASE,
        "6502_interrupt_test.bin.gz",
        "40e457dbe2035c31bcfbcc48abc4d6db661a79e51e1fcc61e7f77298d13d6b0a",
    ),
    (
        INTERRUPT_BASE,
        "6502_interrupt_test.a65",
        "441f93ccc2d39b02a8556f30e1e8727e869da42ff486ea1cd6475e5f22c3c15a",
    ),
    (
        INTERRUPT_BASE,
        "6502_interrupt_test.lst",
        "77f9625886bccd14c563bdadf98aa9d2d3d075e3c24efbfaef4baabde0dd5f9d",
    ),
    (
        DECIMAL_BASE,
        "6502_decimal_test.bin",
        "03798ab778456cc350044fdbe28b4078278648892712b994cdbdda09018674e7",
    ),
    (
        DECIMAL_BASE,
        "6502_decimal_test.a65",
        "6297bb2190f4c635b0a59724ca3471a08d64837aebcabb0b13b5bd011c39fd71",
    ),
    (
        DECIMAL_BASE,
        "6502_decimal_test.lst",
        "439d0826a8882e900325cfc8b30b848991396cfc61a86104d665e1edf62e4a88",
    ),
]
INTERRUPT_IMAGE_SHA256 = (
    "986cfecf0f36a398235b5e936b4ceabef4eccf3d447d5bae3fc2a08c12b5b666"
)


def verify(data: bytes, expected: str, name: str) -> bytes:
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise ValueError(f"{name}: SHA-256 {actual}, expected {expected}")
    return data


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path.home()
        / "Projects/198x/assets/test-suites/6502/dormann-programmes",
    )
    args = parser.parse_args()
    payloads: dict[str, bytes] = {}
    for base, name, digest in SOURCES:
        with urlopen(base + name, timeout=30) as response:
            payloads[name] = verify(response.read(), digest, name)
    payloads["6502_interrupt_test.bin"] = verify(
        gzip.decompress(payloads["6502_interrupt_test.bin.gz"]),
        INTERRUPT_IMAGE_SHA256,
        "6502_interrupt_test.bin",
    )
    # Validate every input before writing anything. Existing loaded fixtures
    # are immutable: a different destination file is an error, not overwritten.
    for name, data in payloads.items():
        path = args.output / name
        if path.exists() and path.read_bytes() != data:
            raise ValueError(f"refusing to overwrite different fixture: {path}")
    args.output.mkdir(parents=True, exist_ok=True)
    for name, data in payloads.items():
        path = args.output / name
        if not path.exists():
            with path.open("xb") as destination:
                destination.write(data)
        print(f"{hashlib.sha256(data).hexdigest()}  {path}")


if __name__ == "__main__":
    main()

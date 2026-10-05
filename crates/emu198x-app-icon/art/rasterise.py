#!/usr/bin/env python3
"""Rasterise the icon SVGs in this folder into the files the build embeds.

Draws nothing itself: the SVGs come from the 198x-ui kit (see README.md).

    python3 crates/emu198x-app-icon/art/rasterise.py

Needs `rsvg-convert` (librsvg); uses `oxipng` to shrink the PNGs if installed.
"""

import pathlib
import shutil
import struct
import subprocess
import tempfile

ART = pathlib.Path(__file__).resolve().parent
CRATE = ART.parent
UI_ICON = CRATE.parent / "emu198x-ui" / "icon"

FULL = ART / "emu198x-icon.svg"
SMALL = ART / "emu198x-icon-small.svg"
MACOS = ART / "emu198x-icon-macos.svg"

# .ico entries: the compact drawing below 32px, the full tile from 32px up.
ICO = [(16, SMALL), (24, SMALL), (32, FULL), (48, FULL), (64, FULL), (128, FULL), (256, FULL)]


def rasterise(svg, px, out):
    subprocess.run(["rsvg-convert", "-w", str(px), "-h", str(px), "-o", str(out), str(svg)], check=True)
    if shutil.which("oxipng"):
        subprocess.run(["oxipng", "-q", "-o", "max", "--strip", "safe", str(out)], check=True)


def write_ico(entries, out):
    """An .ico of PNG-compressed entries (accepted by Windows Vista and later
    and by rc.exe / llvm-rc)."""
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = len(header) + 16 * len(entries)
    directory, blobs = b"", b""
    for px, data in entries:
        dim = 0 if px >= 256 else px  # 0 means 256 in an ICONDIRENTRY
        directory += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    out.write_bytes(header + directory + blobs)


def main():
    # Window and Dock icons, embedded by crates/emu198x-ui/src/icon.rs.
    UI_ICON.mkdir(exist_ok=True)
    rasterise(FULL, 256, UI_ICON / "emu198x-256.png")
    rasterise(SMALL, 32, UI_ICON / "emu198x-small-32.png")
    rasterise(MACOS, 512, UI_ICON / "emu198x-macos-512.png")

    # Shipped beside the binaries in the release archives, for a .desktop entry.
    rasterise(FULL, 256, CRATE / "emu198x.png")

    # Windows .exe resource, compiled by src/lib.rs.
    with tempfile.TemporaryDirectory() as tmp:
        entries = []
        for px, svg in ICO:
            png = pathlib.Path(tmp) / f"{px}.png"
            rasterise(svg, px, png)
            entries.append((px, png.read_bytes()))
        write_ico(entries, CRATE / "emu198x.ico")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Fill apple/Emu198x/LocalContent from a recipe and your own collections.

The Apple TV app plays what is built into it from that git-ignored folder
(see its README). A recipe in apple/content names, for each game, its tape
in a TOSEC set, its box art in a World of Spectrum archive or at a pinned
URL, and its versions with their machines and controls. This script copies
the tapes and ROMs, crops each cover to its front panel and writes the
<Game>.game.json the app reads. It commits nothing and fetches no game
software: tapes and ROMs come only from your folders.

Usage: scripts/bootstrap-apple-local-content.py [recipe]
    --tosec DIR  TOSEC's ZX Spectrum folder
    --wos DIR    the World of Spectrum archive
    --roms DIR   ~/.emu198x/roms, where the desktop hosts keep firmware
    --out DIR    apple/Emu198x/LocalContent
The defaults are where the maintainer keeps these; pass your own.

macOS only: covers are cropped with sips.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from urllib.request import urlopen

REPO = Path(__file__).resolve().parent.parent
DEFAULT_RECIPE = REPO / "apple/content/dizzy.json"
DEFAULT_OUT = REPO / "apple/Emu198x/LocalContent"
DEFAULT_TOSEC = Path("/Volumes/Data/Library/ROMs/TOSEC/Sinclair/ZX Spectrum")
DEFAULT_WOS = Path("/Volumes/Data/WOS-Archive")
DEFAULT_ROMS = Path.home() / ".emu198x/roms"

# The ROMs each machine needs, as SpectrumFirmware.swift accepts them: the
# 48K by knowledge/decisions/test-rom-policy.md, the 128K as MAME's spec128
# driver lists the English images.
FIRMWARE = {
    "spectrum_48k": [
        ("sinclair-zx-spectrum-48k/48.rom", "5ea7c2b824672e914525d1d5c419d71b84a426a2"),
    ],
    "spectrum_128k": [
        ("sinclair-zx-spectrum-128k/128-0.rom", "4f4b11ec22326280bdb96e3baf9db4b4cb1d02c5"),
        ("sinclair-zx-spectrum-128k/128-1.rom", "80080644289ed93d71a1103992a154cc9802b2fa"),
    ],
}

TAPE_EXTENSIONS = {".tap", ".tzx"}


class BootstrapError(Exception):
    pass


def copy_firmware(machines, roms, out):
    for machine in sorted(machines):
        for relative, sha1 in FIRMWARE[machine]:
            source = roms / relative
            if not source.is_file():
                raise BootstrapError(f"{source} is missing; the {machine} needs it")
            data = source.read_bytes()
            if hashlib.sha1(data).hexdigest() != sha1:
                raise BootstrapError(f"{source} is not the unmodified Sinclair ROM")
            (out / source.name).write_bytes(data)


def extract_tape(zip_path, destination):
    """Writes the one tape image in a TOSEC zip to `destination` plus its
    own extension, and returns the file name."""
    if not zip_path.is_file():
        raise BootstrapError(f"{zip_path} is missing")
    with zipfile.ZipFile(zip_path) as archive:
        tapes = [name for name in archive.namelist() if Path(name).suffix.lower() in TAPE_EXTENSIONS]
        if len(tapes) != 1:
            raise BootstrapError(f"{zip_path} holds {len(tapes)} tapes, not one")
        target = destination.with_suffix(Path(tapes[0]).suffix.lower())
        target.write_bytes(archive.read(tapes[0]))
    return target.name


def image_size(path):
    output = subprocess.run(
        ["sips", "-g", "pixelWidth", "-g", "pixelHeight", str(path)],
        check=True, capture_output=True, text=True,
    ).stdout
    values = {}
    for line in output.splitlines()[1:]:
        key, value = line.split(":", 1)
        values[key.strip()] = int(value)
    return values["pixelWidth"], values["pixelHeight"]


def crop_rectangle(cover, width, height):
    """The front panel as (x, y, width, height): a pixel rectangle, the part
    right of a fraction of the width (a cassette inlay's front), or all."""
    if "crop" in cover:
        return tuple(cover["crop"])
    if "left" in cover:
        x = int(width * cover["left"])
        return (x, 0, width - x, height)
    return (0, 0, width, height)


def fetch(url, sha256, directory):
    data = urlopen(url, timeout=60).read()
    if hashlib.sha256(data).hexdigest() != sha256:
        raise BootstrapError(f"{url} has changed since the recipe pinned it")
    path = directory / Path(url).name
    path.write_bytes(data)
    return path


def make_cover(cover, wos, scratch, destination):
    if "wos" in cover:
        source = wos / cover["wos"]
        if not source.is_file():
            raise BootstrapError(f"{source} is missing")
    else:
        source = fetch(cover["url"], cover["sha256"], scratch)
    x, y, width, height = crop_rectangle(cover, *image_size(source))
    subprocess.run(
        ["sips", "-c", str(height), str(width), "--cropOffset", str(y), str(x),
         str(source), "--out", str(destination)],
        check=True, capture_output=True,
    )


def manifest(game, tape_file):
    return {"versions": [dict(version, file=tape_file) for version in game["versions"]]}


def bootstrap(recipe, out, tosec, wos, roms):
    games = json.loads(recipe.read_text())["games"]
    out.mkdir(parents=True, exist_ok=True)
    copy_firmware({v["machine"] for g in games for v in g["versions"]}, roms, out)
    with tempfile.TemporaryDirectory() as scratch:
        for game in games:
            name = game["name"]
            tape = extract_tape(tosec / game["tape"], out / name)
            make_cover(game["cover"], wos, Path(scratch), out / f"{name}.jpg")
            (out / f"{name}.game.json").write_text(json.dumps(manifest(game, tape), indent=1) + "\n")
            print(f"{name}: {tape}, {', '.join(v['name'] for v in game['versions'])}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("recipe", nargs="?", type=Path, default=DEFAULT_RECIPE)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--tosec", type=Path, default=DEFAULT_TOSEC)
    parser.add_argument("--wos", type=Path, default=DEFAULT_WOS)
    parser.add_argument("--roms", type=Path, default=DEFAULT_ROMS)
    args = parser.parse_args()
    try:
        bootstrap(args.recipe, args.out, args.tosec, args.wos, args.roms)
    except BootstrapError as error:
        sys.exit(f"bootstrap: {error}")


if __name__ == "__main__":
    main()

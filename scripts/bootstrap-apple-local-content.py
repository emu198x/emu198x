#!/usr/bin/env python3
"""Fill apple/Emu198x/LocalContent from a recipe and your own collections.

The Apple TV app plays what is built into it from that git-ignored folder
(see its README). A recipe in apple/content names, for each game, its
Spectrum tape in a TOSEC set, its box art in a World of Spectrum archive or
at a pinned URL, and its versions with their machines and controls; a
version on another machine names its own TOSEC images, as `media`. This
script copies the images and ROMs, crops each cover to its front panel and
writes the <Game>.game.json the app reads. It commits nothing and fetches
no game software: images and ROMs come only from your folders.

Usage: scripts/bootstrap-apple-local-content.py [recipe]
    --tosec DIR  the TOSEC set's root
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
DEFAULT_TOSEC = Path("/Volumes/Data/Library/ROMs/TOSEC")
DEFAULT_WOS = Path("/Volumes/Data/WOS-Archive")
DEFAULT_ROMS = Path.home() / ".emu198x/roms"

# The ROMs each machine needs, as MachineModel.swift accepts them: the 48K
# by knowledge/decisions/test-rom-policy.md, the 128K and C64 as MAME's
# spec128 and c64 drivers list them, Kickstart 1.3 as MAME's amiga driver
# and 3.1 (A1200) as WinUAE's ROM list. The consoles need none.
FIRMWARE = {
    "spectrum_48k": [
        ("sinclair-zx-spectrum-48k/48.rom", "5ea7c2b824672e914525d1d5c419d71b84a426a2"),
    ],
    "spectrum_128k": [
        ("sinclair-zx-spectrum-128k/128-0.rom", "4f4b11ec22326280bdb96e3baf9db4b4cb1d02c5"),
        ("sinclair-zx-spectrum-128k/128-1.rom", "80080644289ed93d71a1103992a154cc9802b2fa"),
    ],
    "c64": [
        ("commodore-c64/basic.rom", "79015323128650c742a3694c9429aa91f355905e"),
        ("commodore-c64/kernal.rom", "1d503e56df85a62fee696e7618dc5b4e781df1bb"),
        ("commodore-c64/chargen.rom", "adc7c31e18c7c7413d54802ef2f4193da14711aa"),
    ],
    "amiga_500": [
        ("commodore-amiga/kick13.rom", "891e9a547772fe0c6c19b610baf8bc4ea7fcb785"),
    ],
    "amiga_1200": [
        ("commodore-amiga/kick31a1200.rom", "e21545723fe8374e91342617604f1b3d703094f1"),
    ],
    "nes": [],
    "master_system": [],
    "game_gear": [],
}

TAPE_EXTENSIONS = {".tap", ".tzx"}

# The images a version on each machine takes from a TOSEC zip, and the
# extension it is given, where TOSEC's differs from the app's.
IMAGES = {
    "c64": ({".tap"}, None),
    "nes": ({".nes"}, None),
    "master_system": ({".sms", ".bin"}, ".sms"),
    "game_gear": ({".gg", ".bin"}, ".gg"),
    "amiga_500": ({".adf"}, None),
    "amiga_1200": ({".adf"}, None),
}


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
                raise BootstrapError(f"{source} is not the unmodified published ROM")
            (out / source.name).write_bytes(data)


def extract_image(zip_path, destination, extensions, suffix=None):
    """Writes the one image with one of `extensions` in a TOSEC zip to
    `destination` plus `suffix`, or its own extension, and returns the file
    name."""
    if not zip_path.is_file():
        raise BootstrapError(f"{zip_path} is missing")
    with zipfile.ZipFile(zip_path) as archive:
        images = [name for name in archive.namelist() if Path(name).suffix.lower() in extensions]
        if len(images) != 1:
            raise BootstrapError(f"{zip_path} holds {len(images)} images, not one")
        own = Path(images[0]).suffix.lower()
        target = destination.parent / (destination.name + (suffix or own))
        target.write_bytes(archive.read(images[0]))
    return target.name


def extract_tape(zip_path, destination):
    """Writes the one Spectrum tape in a TOSEC zip, named after the game."""
    return extract_image(zip_path, destination, TAPE_EXTENSIONS)


def extract_media(version, game_name, tosec, out):
    """Writes a version's own images, named after the game and version, and
    returns their file names: a cartridge, a tape, or disks in order."""
    extensions, suffix = IMAGES[version["machine"]]
    sources = version["media"]
    names = []
    for index, source in enumerate(sources):
        stem = f"{game_name} ({version['name']})"
        if len(sources) > 1:
            stem += f" {index + 1}"
        names.append(extract_image(tosec / source, out / stem, extensions, suffix))
    return names


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


def manifest(game, tape_file, media=None):
    """The <Game>.game.json: each version names the game's tape, or its own
    images from `media`, by version name, the first as its file and the
    rest as further disks."""
    versions = []
    for version in game["versions"]:
        entry = {key: value for key, value in version.items() if key != "media"}
        files = (media or {}).get(version["name"])
        if files:
            entry["file"] = files[0]
            if len(files) > 1:
                entry["disks"] = files[1:]
        else:
            entry["file"] = tape_file
        versions.append(entry)
    return {"versions": versions}


def bootstrap(recipe, out, tosec, wos, roms):
    games = json.loads(recipe.read_text())["games"]
    out.mkdir(parents=True, exist_ok=True)
    copy_firmware({v["machine"] for g in games for v in g["versions"]}, roms, out)
    with tempfile.TemporaryDirectory() as scratch:
        for game in games:
            name = game["name"]
            tape = extract_tape(tosec / game["tape"], out / name) if "tape" in game else None
            media = {
                version["name"]: extract_media(version, name, tosec, out)
                for version in game["versions"]
                if "media" in version
            }
            # A game with no cover is a name card in the library.
            if "cover" in game:
                make_cover(game["cover"], wos, Path(scratch), out / f"{name}.jpg")
            (out / f"{name}.game.json").write_text(json.dumps(manifest(game, tape, media), indent=1) + "\n")
            print(f"{name}: {', '.join(v['name'] for v in game['versions'])}")


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

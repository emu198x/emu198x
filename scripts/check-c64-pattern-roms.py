"""Verify the four private firmware inputs for the C64 pattern CI job."""

import argparse
import hashlib
import json
import tempfile
from pathlib import Path

EXPECTED = {
    "basic.rom": (
        8192,
        "89878cea0a268734696de11c4bae593eaaa506465d2029d619c0e0cbccdfa62d",
    ),
    "kernal.rom": (
        8192,
        "83c60d47047d7beab8e5b7bf6f67f80daa088b7a6a27de0d7e016f6484042721",
    ),
    "chargen.rom": (
        4096,
        "fd0d53b8480e86163ac98998976c72cc58d5dd8eb824ed7b829774e74213b420",
    ),
    "1541.rom": (
        16384,
        "326c289c38753323d7e8167897447cf61ef35189d82eb8d75210ece949adda7c",
    ),
}


def verify(directory: Path) -> dict[str, dict[str, int | str]]:
    result = {}
    for name, (length, digest) in EXPECTED.items():
        data = (directory / name).read_bytes()
        actual = hashlib.sha256(data).hexdigest()
        if len(data) != length or actual != digest:
            raise ValueError(f"{name}: unexpected size or SHA-256")
        result[name] = {"bytes": len(data), "sha256": actual}
    return result


def controls(directory: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="c64-rom-controls-") as temporary:
        target = Path(temporary)
        try:
            verify(target)
        except FileNotFoundError as error:
            if Path(error.filename).name != "basic.rom":
                raise
        else:
            raise ValueError("Missing firmware was accepted")
        for name in EXPECTED:
            (target / name).write_bytes((directory / name).read_bytes())
        basic = bytearray((target / "basic.rom").read_bytes())
        basic[0] ^= 1
        (target / "basic.rom").write_bytes(basic)
        try:
            verify(target)
        except ValueError as error:
            if str(error) != "basic.rom: unexpected size or SHA-256":
                raise
        else:
            raise ValueError("Wrong firmware was accepted")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.unlink(missing_ok=True)
    images = verify(args.rom_dir)
    controls(args.rom_dir)
    args.output.write_text(
        json.dumps(
            {
                "firmware": images,
                "missing_rom_rejected": True,
                "wrong_rom_rejected": True,
            },
            indent=2,
        )
        + "\n"
    )
    print("Four firmware identities verified; missing/wrong-ROM controls rejected")


if __name__ == "__main__":
    main()

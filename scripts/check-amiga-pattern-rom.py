"""Verify the private Kickstart input for the Paula pattern CI job."""

import argparse
import hashlib
import json
import tempfile
from pathlib import Path

ROM_NAME = "kickstart-1.3-r34.5.rom"
ROM_SIZE = 262144
ROM_SHA256 = "ee05862d8102a08436ac4056da7d549db31625c7d47b24dfb7b3c9a5c113ca53"


def verify(path: Path) -> dict[str, int | str]:
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if len(data) != ROM_SIZE or digest != ROM_SHA256:
        raise ValueError("Kickstart: unexpected size or SHA-256")
    return {"bytes": len(data), "sha256": digest}


def controls(path: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="amiga-rom-controls-") as temporary:
        target = Path(temporary) / ROM_NAME
        try:
            verify(target)
        except FileNotFoundError:
            pass
        else:
            raise ValueError("Missing firmware was accepted")
        data = bytearray(path.read_bytes())
        data[0] ^= 1
        target.write_bytes(data)
        try:
            verify(target)
        except ValueError as error:
            if str(error) != "Kickstart: unexpected size or SHA-256":
                raise
        else:
            raise ValueError("Wrong firmware was accepted")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.unlink(missing_ok=True)
    identity = verify(args.rom)
    controls(args.rom)
    args.output.write_text(
        json.dumps(
            {
                "firmware": {ROM_NAME: identity},
                "missing_rom_rejected": True,
                "wrong_rom_rejected": True,
            },
            indent=2,
        )
        + "\n"
    )
    print("Kickstart identity verified; missing/wrong-ROM controls rejected")


if __name__ == "__main__":
    main()

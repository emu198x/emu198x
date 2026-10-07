#!/usr/bin/env python3
"""Capture the VICE xvic reference frames for the VIC-20 VIC-I survey.

The survey (`knowledge/processes/vic20-vici-vice-survey.md`) compares
Emu198x's VIC-I output with VICE's, frame for frame. This script produces
VICE's half: one full-raster screenshot per case in
`test-data/commodore/vic-20/vici-vice-survey/cases-v1.json`, plus sixteen
palette-calibration frames per video standard, written below
`<fixture-dir>/references/`.

Both emulators reach the same point the same way. Each boots from power-on;
at the first execution of `$E5EA` (the KERNAL editor's wait for a key, the
first instruction boundary at which BASIC is ready) a case's program is
copied into RAM at its load address, BASIC's end-of-program pointers are set
past it and `RUN` + RETURN is queued in the keyboard buffer, followed by any
further keys the case lists for the program to read. The frame is
taken at an exact cycle count from power-on: VICE stops at `-limitcycles`
and writes `-exitscreenshot`; the Rust survey runs the same number of
cycles. Injection uses the VICE monitor (a checkpoint on `$E5EA` whose
command plays back a file of monitor commands), not VICE's autostart, whose
timing is VICE's own and, by default, randomised.

VICE's settings are fixed: `-default` on an empty configuration file, so no
user `vicerc` leaks in; `-VICborders 2` (the whole raster, every line and
every cycle); `-VICfilter 0` (no CRT emulation). VICE renders each VIC-I
pixel two host pixels wide, so the PNG is 568 x 312 for PAL.

VICE's palette is not Emu198x's, and VICE adjusts colours on the way out, so
the survey never compares RGB values. It classifies VICE's pixels by exact
match against VICE's own rendering of each of the sixteen colours, captured
here: palette-NN.png is a frame of a program that sets background colour NN
(and border colour NN & 7) and spins.

Default mode captures every frame into the fixture directory and checks the
result against the SHA-256 values pinned in the cases file, failing on any
difference. `--update` rewrites the pinned values instead; review the diff
before committing it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CASES = REPO / "test-data/commodore/vic-20/vici-vice-survey/cases-v1.json"
DEFAULT_FIXTURE = Path.home() / ".emu198x/test-suites/vic20"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def calibration_program(colour: int) -> bytes:
    """`10 SYS4109`, then LDA #v / STA $900F / JMP *, where v gives background
    `colour`, border `colour & 7` and normal (not reversed) video."""
    value = (colour << 4) | 0x08 | (colour & 0x07)
    basic = bytes([0x0B, 0x10, 0x0A, 0x00, 0x9E]) + b"4109" + bytes([0x00, 0x00, 0x00])
    code = bytes([0xA9, value, 0x8D, 0x0F, 0x90, 0x4C, 0x12, 0x10])
    program = b"\x01\x10" + basic + code
    assert len(program) - 2 + 0x1001 == 0x1015, "code must sit at $100D"
    return program


def resolve_program(case: dict, fixture: Path) -> Path | None:
    program = case.get("program")
    if program is None:
        return None
    root = {"fixture": fixture, "repo": REPO}[program["root"]]
    return root / program["path"]


def monitor_files(work: Path, program: Path, injection_pc: str, keys: str) -> Path:
    """Write the two monitor-command files that inject `program` at the
    first execution of `injection_pc`, with `keys` queued in the keyboard
    buffer, and return the one VICE starts with."""
    image = program.read_bytes()
    load = image[0] | image[1] << 8
    end = load + len(image) - 2
    lo, hi = end & 0xFF, end >> 8
    if not 0 < len(keys) <= 10:
        raise SystemExit(f"{len(keys)} keys do not fit the 10-key buffer")
    typed = " ".join(f"{ord(key):02x}" for key in keys)
    inject = work / "inject.mon"
    inject.write_text(
        "del 1\n"
        f'load "{program}" 0\n'
        f"> 002d {lo:02x} {hi:02x} {lo:02x} {hi:02x} {lo:02x} {hi:02x}\n"
        f"> 0277 {typed}\n"
        f"> 00c6 {len(keys):02x}\n"
    )
    start = work / "start.mon"
    start.write_text(f'break {injection_pc}\ncommand 1 "playback \\"{inject}\\""\nx\n')
    return start


def run_xvic(
    xvic: str,
    manifest: dict,
    model: str,
    memory: str,
    program: Path | None,
    frame: int,
    out: Path,
    keys: str | None = None,
) -> None:
    standard = manifest["models"][model]
    with tempfile.TemporaryDirectory(prefix="vic20-vici-vice-") as tmp:
        work = Path(tmp)
        config = work / "empty.vicerc"
        config.write_text("")
        args = [xvic, "-config", str(config), *manifest["vice"]["args"]]
        args += ["-model", standard["vice_model"], "-memory", memory]
        args += ["-limitcycles", str(frame * standard["cycles_per_frame"])]
        args += ["-exitscreenshot", str(out)]
        if program is not None:
            start = monitor_files(
                work, program, manifest["injection"]["pc"], keys or manifest["injection"]["keys"]
            )
            args += ["-moncommands", str(start), "-nativemonitor"]
        if out.exists():
            out.unlink()
        result = subprocess.run(
            args, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=600
        )
        log = result.stdout + result.stderr
        if "cycle limit reached" not in log or not out.exists():
            raise SystemExit(f"xvic did not reach its cycle limit for {out.name}:\n{log}")
        if program is not None and "Stop on  exec" not in log:
            raise SystemExit(f"xvic never reached the injection point for {out.name}:\n{log}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--fixture-dir",
        type=Path,
        default=Path(os.environ.get("EMU198X_VIC20_VICE_SURVEY_DIR", DEFAULT_FIXTURE)),
    )
    parser.add_argument("--xvic", default=shutil.which("xvic") or "xvic")
    parser.add_argument("--update", action="store_true", help="rewrite the pinned SHA-256 values")
    parser.add_argument("--case", action="append", help="capture only these case ids")
    args = parser.parse_args()

    manifest = json.loads(CASES.read_text())
    fixture: Path = args.fixture_dir
    references = fixture / "references"
    failures: list[str] = []

    def pin(record: dict, key: str, path: Path) -> None:
        digest = sha256(path)
        if args.update:
            record[key] = digest
        elif record.get(key) != digest:
            failures.append(f"{path.relative_to(fixture)}: {digest}, manifest pins {record.get(key)}")

    for case in manifest["cases"]:
        program = resolve_program(case, fixture)
        if program is not None:
            if not program.is_file():
                raise SystemExit(f"{case['id']}: program {program} is not staged")
            pin(case["program"], "sha256", program)

    selected = set(args.case or [])
    for model, calibration in manifest["palette_calibration"].items():
        if selected:
            break
        directory = references / model
        directory.mkdir(parents=True, exist_ok=True)
        for colour in range(16):
            out = directory / f"palette-{colour:02d}.png"
            with tempfile.TemporaryDirectory(prefix="vic20-vici-palette-") as tmp:
                program = Path(tmp) / f"palette-{colour:02d}.prg"
                program.write_bytes(calibration_program(colour))
                run_xvic(args.xvic, manifest, model, "none", program, calibration["capture_frame"], out)
            pin(calibration["sha256"], f"{colour:02d}", out)
            print(f"captured {out.relative_to(fixture)}")

    for case in manifest["cases"]:
        if selected and case["id"] not in selected:
            continue
        directory = references / case["model"]
        directory.mkdir(parents=True, exist_ok=True)
        out = directory / f"{case['id']}.png"
        run_xvic(
            args.xvic,
            manifest,
            case["model"],
            case["memory"],
            resolve_program(case, fixture),
            case["capture_frame"],
            out,
            case.get("keys"),
        )
        pin(case, "reference_sha256", out)
        print(f"captured {out.relative_to(fixture)}")

    if args.update:
        CASES.write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"updated {CASES.relative_to(REPO)}")
        return 0
    if failures:
        print("captures differ from the pinned manifest:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("every capture matches the pinned manifest")
    return 0


if __name__ == "__main__":
    sys.exit(main())

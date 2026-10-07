#!/usr/bin/env python3
"""Verify retained static references; separate comparator consensus from phase."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageSequence

PRODUCERS = ("fs-uae-5.0.7-f362278c", "copperline-0.13.0-eec5806")
CASES = (
    "fixed-control",
    "ecsena-gate",
    "extblken-gate",
    "blanken-path",
    "programmed-central",
    "programmed-wrap",
    "programmed-equal",
)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def runs(levels: list[bool], origin: int) -> list[dict[str, int]]:
    result = []
    start = None
    for x, blank in enumerate([*levels, False], origin):
        if blank and start is None:
            start = x
        elif not blank and start is not None:
            result.append({"start": start, "end_exclusive": x})
            start = None
    return result


def requalify(root: Path) -> dict:
    observations = {}
    package_hashes = {}
    registered_hashes = {
        PRODUCERS[
            0
        ]: "def3500455db110dd7027454dd042e754124ab2910477ae5e2e3459c83e20be1",
        PRODUCERS[
            1
        ]: "2221be40ca162c2bf03b87f81b178cbc2ae9ac05d495ebb135f2b990f1d44e78",
    }
    for producer in PRODUCERS:
        base = root / producer
        package_path = base / "package-v1.json"
        package_hashes[producer] = digest(package_path)
        if package_hashes[producer] != registered_hashes[producer]:
            raise ValueError(f"{producer}: unregistered package manifest")
        package = json.loads(package_path.read_text())
        selected = [run for run in package["runs"] if run["case_id"] in CASES]
        keys = {(run["profile"], run["case_id"]) for run in selected}
        if (
            keys != {(profile, case) for profile in ("ecs", "aga") for case in CASES}
            or len(selected) != 14
        ):
            raise ValueError(f"{producer}: incomplete or duplicate reference matrix")
        for run in selected:
            for field in ("record", "capture", "capture_manifest", "run_log"):
                if digest(base / run[f"{field}_file"]) != run[f"{field}_sha256"]:
                    raise ValueError(
                        f"{producer}: changed {field} for {run['case_id']}"
                    )
            record = json.loads((base / run["record_file"]).read_text())
            coordinate = record["normalization"]["beam_coordinate"]
            origin = coordinate["horizontal_origin_sample"]
            expected_origin = -184 if producer == PRODUCERS[0] else -196
            if origin != expected_origin or coordinate["sample_beam_line"] != 128:
                raise ValueError("unregistered reference coordinate")
            with Image.open(base / run["capture_file"]) as image:
                frames = [
                    frame.convert("RGBA").tobytes()
                    for frame in ImageSequence.Iterator(image)
                ]
                width, height = image.size
            if (width, height) != (
                (756, 576) if producer == PRODUCERS[0] else (716, 570)
            ):
                raise ValueError("unregistered reference dimensions")
            if len(frames) != 3 or len(set(frames)) != 1:
                raise ValueError(
                    "reference must contain three identical adjacent frames"
                )
            if (
                hashlib.sha256(b"".join(frames)).hexdigest()
                != run["decoded_pixel_sha256"]
            ):
                raise ValueError("decoded reference hash disagrees")
            row = coordinate["sample_row"]
            rgb = [
                tuple(frames[0][(row * width + x) * 4 : (row * width + x) * 4 + 3])
                for x in range(width)
            ]
            guard = int(record["observations"]["guard_color_word"], 16)
            colour = tuple(((guard >> shift) & 15) * 17 for shift in (8, 4, 0))
            if any(pixel not in ((0, 0, 0), colour) for pixel in rgb):
                raise ValueError("unclassified reference colour")
            observations[(producer, run["profile"], run["case_id"])] = (rgb, origin)
    output = []
    for profile in ("ecs", "aga"):
        for case in CASES:
            fs, fs_origin = observations[(PRODUCERS[0], profile, case)]
            copper, copper_origin = observations[(PRODUCERS[1], profile, case)]
            # Compare declared HB-register coordinates only. Copperline applies
            # a post-render mask; that agreement cannot prove signal latency.
            fs_semantic = [
                pixel == (0, 0, 0) for pixel in fs[196 + fs_origin : 912 + fs_origin]
            ]
            copper_semantic = [
                pixel == (0, 0, 0)
                for pixel in copper[196 + copper_origin : 912 + copper_origin]
            ]
            consensus = case in (
                "fixed-control",
                "programmed-central",
                "programmed-wrap",
                "programmed-equal",
            ) or (profile == "ecs" and case == "blanken-path")
            agreement = fs_semantic == copper_semantic
            if (
                len(fs_semantic) != 716
                or len(copper_semantic) != 716
                or agreement != consensus
            ):
                raise ValueError(
                    f"{profile}/{case}: comparator agreement classification changed"
                )
            if any(fs[x] != fs[x + 1] for x in range(2, 756, 2)):
                raise ValueError("reference does not preserve lores sample pairs")
            # Independently traced origins: native=Denise88, FS=Denise91.
            # Keep FS storage [0,2) excluded, and retain every other sample.
            native_runs = runs([fs[x] == (0, 0, 0) for x in range(2, 756, 2)], 4)
            output.append(
                {
                    "profile": profile,
                    "case": case,
                    "comparator_consensus": consensus,
                    "native_black_runs": native_runs,
                }
            )
    return {
        "evidence_scope": "UAE absolute phase; cross-family comparator semantics only",
        "package_sha256": package_hashes,
        "compared_reference_frames": 84,
        "native_lores_interval": [4, 381],
        "runs": output,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = (
        Path(__file__).resolve().parents[2]
        / "test-data/commodore/amiga/programmable-hblank/references"
    )
    result = requalify(root)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(
        "Verified 84 retained fields: nine comparator agreements, five disagreements; fourteen UAE phase observations."
    )


if __name__ == "__main__":
    main()

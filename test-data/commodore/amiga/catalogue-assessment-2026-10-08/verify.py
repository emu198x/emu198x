"""Verify retained observations without private disks, ROMs or audio files."""

import hashlib
import json
import re
from pathlib import Path

import tomllib
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parent


def frame(revision: str, entry: str) -> Image.Image:
    return Image.open(ROOT / "frames" / revision / f"{entry}.png").convert("RGB")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def main() -> None:
    rows = json.loads((ROOT / "measurements.json").read_text())
    require(len(rows) == 35, "expected five observations for each of seven entries")
    require(
        len({(r["revision"], r["entry"]) for r in rows}) == 35, "duplicate observation"
    )
    for row in rows:
        path = ROOT / "frames" / row["revision"] / f"{row['entry']}.png"
        require(
            hashlib.sha256(path.read_bytes()).hexdigest() == row["frame_png_sha256"],
            f"changed frame: {path}",
        )
        require(list(Image.open(path).size) == row["frame_size"], "frame size changed")
        require(row["stereo_frames"] > 0, "empty audio observation")
        require(row["clipped_samples"] == 0, "observed endpoint clipping")

    # A fixed edge correction must not change any desktop pixel.
    before = frame("1cead4e8", "workbench-1.3-desktop")
    after = frame("pointer-fixed", "workbench-1.3-desktop")
    require(before.size == after.size == (768, 576), "unexpected OCS geometry")
    changed = []
    for index, (old, new) in enumerate(
        zip(before.get_flattened_data(), after.get_flattened_data(), strict=True)
    ):
        if old == new:
            continue
        x, y = index % 768, index // 768
        require((x < 8 or x >= 760) and y < 574, "desktop changed outside blanking")
        require(old == (0, 85, 170) and new == (0, 0, 0), "unexpected edge colour")
        changed.append((x, y))
    require(len(changed) == 9184, "missing OCS edge observations")

    # Counter-traced one-lores-tick correction, not fitted image alignment.
    before = frame("1cead4e8", "workbench-3.1-desktop")
    after = frame("pointer-fixed", "workbench-3.1-desktop")
    require(before.size == (768, 576) and after.size == (1536, 576), "Lisa geometry")
    before = before.resize((1536, 576), Image.Resampling.NEAREST)
    difference = ImageChops.difference(
        before.crop((4, 0, 1536, 576)), after.crop((0, 0, 1532, 576))
    )
    pointer = edges = 0
    for index, value in enumerate(difference.get_flattened_data()):
        if value == (0, 0, 0):
            continue
        x, y = index % 1532, index // 1532
        if 164 <= x <= 207 and 38 <= y <= 59:
            pointer += 1
        else:
            require(x < 16 or x >= 1524 or y < 2, "unclassified Lisa pixel")
            edges += 1
    require(pointer == 368 and edges == 16792, "Lisa classification changed")

    for entry in ["1943", "alien-syndrome-ntsc"]:
        require(
            frame("1cead4e8", entry).tobytes()
            == frame("pointer-fixed", entry).tobytes(),
            f"unexpected visual change: {entry}",
        )
    for row in rows:
        if row["entry"] == "1943":
            require(row["nonzero_samples"] == 0, "1943 observation is not silent")
            expected = 95846 if row["revision"] == "1cead4e8" else 95845
            require(row["stereo_frames"] == expected, "1943 window length changed")
    reference = sorted((ROOT / "reference-sota").glob("field-*.json"))
    require(len(reference) == 7, "missing independent reference observations")
    for path, field in zip(reference, range(5500, 6101, 100), strict=True):
        metadata = json.loads(path.read_text())
        require(metadata["core_field"] == field, "reference field discontinuity")
        require(
            Image.open(path.with_suffix(".png")).size
            == (metadata["width"], metadata["height"]),
            "reference geometry changed",
        )
    before_manifest = tomllib.loads((ROOT / "manifest-before.toml").read_text())
    after_manifest = tomllib.loads((ROOT / "manifest-after.toml").read_text())
    require(before_manifest["system"] == after_manifest["system"], "firmware changed")
    require(
        len(before_manifest["entry"]) == len(after_manifest["entry"]) == 10,
        "catalogue entry count changed",
    )
    for old, new in zip(before_manifest["entry"], after_manifest["entry"], strict=True):
        old["boot"]["frame_hash"] = new["boot"]["frame_hash"]
        old["audio"]["hash"] = new["audio"]["hash"]
        require(old == new, "manifest changed beyond the reviewed hashes")
    logs = "\n".join((ROOT / f"final-shard-{i}.log").read_text() for i in [1, 2])
    expected = {entry["id"] for entry in after_manifest["entry"]}
    for tag in ["PASS", "SNAP-PASS"]:
        passed = re.findall(rf"^\[{tag}\] ([^\s]+)", logs, re.MULTILINE)
        require(len(passed) == 10 and set(passed) == expected, f"incomplete {tag} gate")
    require("[FAIL]" not in logs and "[SNAP-FAIL]" not in logs, "final gate failed")
    require(logs.count("5 entries run, 0 failure(s)") == 2, "missing completed shards")
    print(
        "35 observations verified; exact OCS/Lisa classification; seven reference fields"
    )
    print(
        "Only manifest hashes changed; ten catalogue and ten snapshot passes verified"
    )


if __name__ == "__main__":
    main()

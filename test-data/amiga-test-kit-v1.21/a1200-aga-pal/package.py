#!/usr/bin/env python3
"""Package and verify the FS-UAE A1200 Amiga Test Kit reference."""

from __future__ import annotations

import argparse
import binascii
import hashlib
import json
import re
import struct
import sys
import zlib
from pathlib import Path
from typing import Any


PACKAGE_DIR = Path(__file__).resolve().parent
REPO_ROOT = PACKAGE_DIR.parents[2]
ADAPTER_DIR = REPO_ROOT / "tools/fs-uae-test-kit-video-capture"

SOURCE_REVISION = "f362278ccd4c60991caac3b4d240d4a3f751bea2"
BINARY_SHA256 = "5c3d9e35d100445a5603c5f86a19cc431a7363828053d4ede7d260c2c5d6899f"
PATCH_SHA256 = "6116765eab7036cf756cb3212968675c9d1ca3ef327b8da3e4d194f05ffbb767"
ADF_SHA256 = "abe7426c93619a7bb61ce10e3e66a4747fcaf22acd1d1876310033faa700ad28"
FIRMWARE_SHA256 = "6d43840d4099a74170ea0f0425b6257c3891ebcaa39c4d1840075a9ab22b5707"

CAPTURE_TOOLS = {
    "capture.sh": "511cfed52f2b5d8a03a3d335bc144fbee59d2624f94c734e828c135b566eca28",
    "capture_manifest.py": (
        "896d310b9eecdcb09d67d29436e3ee1a389bde7385d595fab6048a13ee3a076e"
    ),
    "config.uae.in": (
        "cf8f5bdb01142cfe08c271158a0e1253ddef700f03b52a9f6f67698fc2648745"
    ),
    "Portable.ini": (
        "f6ea7ad62b30f5b1d3092081990d41206c60aea7dcc29379a2977e89c4d994f0"
    ),
    "fs-uae-5.0.7-test-kit-video-capture.patch": PATCH_SHA256,
}

RAW_WIDTH = 756
RAW_HEIGHT = 576
RAW_STRIDE = RAW_WIDTH * 4
PRODUCER_CROP_X = 2
PRODUCER_CROP_Y = 0
CROP_WIDTH = 752
CROP_HEIGHT = 572
VERTICAL_DECIMATION = 2
CANONICAL_WIDTH = 752
CANONICAL_HEIGHT = 286
RUNTIME_CROP_X = 8
RUNTIME_CROP_Y = 2

CASE_SPECS: tuple[dict[str, Any], ...] = (
    {
        "id": "gradients",
        "navigation": ["F6", "F1"],
        "settle_fields": 150,
        "behaviour": "static",
        "first_capture_field": 808,
        "references": (("static", "gradients.png", 0),),
    },
    {
        "id": "static-checkerboard",
        "navigation": ["F6", "F2"],
        "settle_fields": 100,
        "behaviour": "static",
        "first_capture_field": 758,
        "references": (("static", "static-checkerboard.png", 0),),
    },
    {
        "id": "alternating-checkerboard",
        "navigation": ["F6", "F3"],
        "settle_fields": 100,
        "behaviour": "alternating",
        "first_capture_field": 758,
        "references": (
            ("a", "alternating-checkerboard-phase-a.png", 0),
            ("b", "alternating-checkerboard-phase-b.png", 1),
        ),
    },
    {
        "id": "ebu-bars",
        "navigation": ["F6", "F4", "F6"],
        "settle_fields": 100,
        "behaviour": "static",
        "first_capture_field": 812,
        "references": (("static", "ebu-bars.png", 0),),
    },
    {
        "id": "dots",
        "navigation": ["F6", "F5"],
        "settle_fields": 100,
        "behaviour": "static",
        "first_capture_field": 758,
        "references": (("static", "dots.png", 0),),
    },
    {
        "id": "crosshatch",
        "navigation": ["F6", "F6"],
        "settle_fields": 100,
        "behaviour": "static",
        "first_capture_field": 758,
        "references": (("static", "crosshatch.png", 0),),
    },
)

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: expected a JSON object")
    return value


def require_equal(actual: Any, expected: Any, context: str) -> None:
    if actual != expected:
        raise ValueError(f"{context}: got {actual!r}, expected {expected!r}")


def adapter_hashes() -> dict[str, str]:
    return {name: sha256_file(ADAPTER_DIR / name) for name in CAPTURE_TOOLS}


def parse_hash_manifest(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for line_number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        parts = line.split("  ", maxsplit=1)
        if len(parts) != 2 or SHA256_RE.fullmatch(parts[0]) is None:
            raise ValueError(f"{path}:{line_number}: malformed SHA-256 record")
        name = Path(parts[1]).name
        if name in result:
            raise ValueError(f"{path}: duplicate basename {name}")
        result[name] = parts[0]
    return result


def expected_execution(spec: dict[str, Any]) -> dict[str, Any]:
    first = spec["first_capture_field"]
    return {
        "behaviour": spec["behaviour"],
        "boot_fields": 600,
        "captured_core_fields": [first, first + 1, first + 2],
        "final_settle_fields": spec["settle_fields"],
        "first_capture_field": first,
        "inter_key_fields": 50,
        "key_hold_fields": 3,
        "key_release_settle_fields": 1,
        "navigation": spec["navigation"],
    }


def validate_run(
    run_root: Path, spec: dict[str, Any]
) -> tuple[list[bytes], dict[str, Any]]:
    run_dir = run_root / spec["id"]
    manifest_path = run_dir / "capture-manifest.json"
    manifest = read_json(manifest_path)

    require_equal(manifest.get("schema_version"), "1.0.0", "capture schema")
    require_equal(manifest["capture"]["case_id"], spec["id"], "capture case")
    require_equal(manifest["suite"]["name"], "Amiga Test Kit", "suite name")
    require_equal(manifest["suite"]["version"], "1.21", "suite version")
    require_equal(manifest["suite"]["adf_sha256"], ADF_SHA256, "suite ADF")
    require_equal(
        manifest["suite"]["source_commit"],
        "9477599d1611da2326f43532dbe563c2848e308b",
        "suite source",
    )
    require_equal(
        manifest["machine"],
        {
            "chip_ram_bytes": 2_097_152,
            "chipset": "AGA",
            "cpu": "68EC020",
            "expansion_ram_bytes": 0,
            "firmware_sha256": FIRMWARE_SHA256,
            "model": "Commodore Amiga A1200",
            "region": "PAL",
        },
        "machine",
    )
    require_equal(manifest["execution"], expected_execution(spec), "execution")

    producer = manifest["producer"]
    require_equal(producer["product"], "FS-UAE", "producer product")
    require_equal(producer["version"], "5.0.7", "producer version")
    require_equal(producer["revision"], SOURCE_REVISION, "producer revision")
    require_equal(producer["uae_base_version"], "WinUAE 6.0.1", "UAE base")
    require_equal(producer["implementation_family"], "UAE", "producer family")
    require_equal(producer["binary_sha256"], BINARY_SHA256, "producer binary")
    require_equal(producer["capture_patch_sha256"], PATCH_SHA256, "producer patch")
    binary_path = Path(producer["binary_file"])
    require_equal(sha256_file(binary_path), BINARY_SHA256, "binary file")

    recorded_tools = dict(manifest["capture_tools"])
    recorded_tools.pop("directory")
    require_equal(recorded_tools, CAPTURE_TOOLS, "capture tools")
    require_equal(adapter_hashes(), CAPTURE_TOOLS, "tracked capture tools")

    inputs = manifest["inputs"]
    require_equal(inputs["firmware"]["sha256"], FIRMWARE_SHA256, "firmware record")
    require_equal(
        sha256_file(Path(inputs["firmware"]["file"])),
        FIRMWARE_SHA256,
        "firmware file",
    )
    require_equal(inputs["test_kit_adf"]["sha256"], ADF_SHA256, "ADF record")
    require_equal(
        sha256_file(Path(inputs["test_kit_adf"]["file"])), ADF_SHA256, "ADF file"
    )
    require_equal(inputs["unchanged_during_capture"], True, "input stability")
    require_equal(
        (run_dir / "inputs-before.sha256").read_bytes(),
        (run_dir / "inputs-after.sha256").read_bytes(),
        "before and after input manifests",
    )
    require_equal(
        sha256_file(run_dir / "config.uae"),
        inputs["configuration"]["sha256"],
        "configuration file",
    )
    require_equal(
        sha256_file(Path(inputs["runtime_portable"]["file"])),
        CAPTURE_TOOLS["Portable.ini"],
        "runtime Portable.ini",
    )

    files = manifest["files"]
    require_equal(files["frontend_wait_status"] in (0, 143), True, "wait status")
    require_equal(
        sha256_file(run_dir / "run.stdout"), files["run_log_sha256"], "run log"
    )
    require_equal(
        sha256_file(run_dir / "capture.sha256"),
        files["capture_hash_manifest_sha256"],
        "capture hash manifest",
    )

    raw_capture = manifest["raw_capture"]
    require_equal(raw_capture["width"], RAW_WIDTH, "raw width")
    require_equal(raw_capture["height"], RAW_HEIGHT, "raw height")
    require_equal(raw_capture["pixel_format"], "BGRA8888", "raw format")
    require_equal(raw_capture["packed_stride_bytes"], RAW_STRIDE, "raw stride")
    require_equal(raw_capture["producer_stride_bytes"], 8192, "producer stride")

    field_records = raw_capture["fields"]
    require_equal(len(field_records), 3, "raw field count")
    capture_hashes = parse_hash_manifest(run_dir / "capture.sha256")
    raw_frames: list[bytes] = []
    raw_hashes: list[str] = []
    for index, field in enumerate(field_records):
        core_field = spec["first_capture_field"] + index
        raw_path = run_dir / "capture" / field["raw_file"]
        metadata_path = run_dir / "capture" / field["metadata_file"]
        require_equal(field["core_field"], core_field, "field label")
        require_equal(raw_path.name, f"field-{core_field:06d}.bgra", "raw name")
        require_equal(metadata_path.stem, raw_path.stem, "metadata name")
        raw = raw_path.read_bytes()
        require_equal(len(raw), RAW_WIDTH * RAW_HEIGHT * 4, "raw byte count")
        require_equal(sha256_bytes(raw), field["raw_sha256"], "raw hash")
        require_equal(
            capture_hashes[raw_path.name], field["raw_sha256"], "listed raw hash"
        )
        require_equal(
            sha256_file(metadata_path), field["metadata_sha256"], "metadata hash"
        )
        require_equal(
            capture_hashes[metadata_path.name],
            field["metadata_sha256"],
            "listed metadata hash",
        )
        require_equal(read_json(metadata_path), field["metadata"], "field metadata")
        raw_frames.append(raw)
        raw_hashes.append(field["raw_sha256"])

    if spec["behaviour"] == "static":
        require_equal(len(set(raw_hashes)), 1, "static field stability")
        require_equal(raw_capture["temporal_relation"], "all-byte-identical", "timing")
    else:
        require_equal(raw_hashes[0] == raw_hashes[2], True, "alternating repeat")
        require_equal(raw_hashes[0] != raw_hashes[1], True, "alternating phases")
        require_equal(
            raw_capture["temporal_relation"],
            "first-equals-third-and-differs-from-second",
            "timing",
        )

    summary = {
        "capture_manifest_sha256": sha256_file(manifest_path),
        "captured_at_utc": manifest["capture"]["captured_at_utc"],
        "operator": manifest["capture"]["operator"],
        "host": manifest["capture"]["host"],
        "configuration_sha256": inputs["configuration"]["sha256"],
        "run_log_sha256": files["run_log_sha256"],
        "inputs_before_sha256": inputs["before_manifest_sha256"],
        "inputs_after_sha256": inputs["after_manifest_sha256"],
        "frontend_wait_status": files["frontend_wait_status"],
        "captured_core_fields": expected_execution(spec)["captured_core_fields"],
        "raw_sha256": raw_hashes,
    }
    return raw_frames, summary


def canonical_rgb(raw: bytes) -> tuple[bytes, list[int]]:
    canonical = bytearray()
    alpha_values: set[int] = set()
    for canonical_y in range(CANONICAL_HEIGHT):
        source_y = PRODUCER_CROP_Y + canonical_y * VERTICAL_DECIMATION
        row_start = source_y * RAW_STRIDE + PRODUCER_CROP_X * 4
        row_stop = row_start + CROP_WIDTH * 4
        first_row = raw[row_start:row_stop]
        second_start = row_start + RAW_STRIDE
        second_row = raw[second_start : second_start + CROP_WIDTH * 4]
        if first_row != second_row:
            raise ValueError(f"raw row pair {source_y}/{source_y + 1} differs")
        for offset in range(0, len(first_row), 4):
            blue, green, red, alpha = first_row[offset : offset + 4]
            canonical.extend((red, green, blue))
            alpha_values.add(alpha)
    expected_bytes = CANONICAL_WIDTH * CANONICAL_HEIGHT * 3
    require_equal(len(canonical), expected_bytes, "canonical RGB byte count")
    return bytes(canonical), sorted(alpha_values)


def png_chunk(chunk_type: bytes, payload: bytes) -> bytes:
    checksum = binascii.crc32(chunk_type + payload) & 0xFFFF_FFFF
    return (
        struct.pack(">I", len(payload))
        + chunk_type
        + payload
        + struct.pack(">I", checksum)
    )


def encode_png(rgb: bytes) -> bytes:
    stride = CANONICAL_WIDTH * 3
    filtered = b"".join(
        b"\0" + rgb[row * stride : (row + 1) * stride]
        for row in range(CANONICAL_HEIGHT)
    )
    return b"".join(
        (
            b"\x89PNG\r\n\x1a\n",
            png_chunk(
                b"IHDR",
                struct.pack(
                    ">IIBBBBB",
                    CANONICAL_WIDTH,
                    CANONICAL_HEIGHT,
                    8,
                    2,
                    0,
                    0,
                    0,
                ),
            ),
            png_chunk(b"IDAT", zlib.compress(filtered, level=9)),
            png_chunk(b"IEND", b""),
        )
    )


def decode_png(path: Path) -> bytes:
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError(f"{path}: invalid PNG signature")
    offset = 8
    chunks: list[tuple[bytes, bytes]] = []
    while offset < len(data):
        if offset + 12 > len(data):
            raise ValueError(f"{path}: truncated PNG chunk")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        checksum = struct.unpack(">I", data[offset + 8 + length : offset + 12 + length])[0]
        if binascii.crc32(chunk_type + payload) & 0xFFFF_FFFF != checksum:
            raise ValueError(f"{path}: PNG checksum mismatch")
        chunks.append((chunk_type, payload))
        offset += 12 + length
    require_equal([kind for kind, _ in chunks], [b"IHDR", b"IDAT", b"IEND"], "PNG chunks")
    header = struct.unpack(">IIBBBBB", chunks[0][1])
    require_equal(
        header,
        (CANONICAL_WIDTH, CANONICAL_HEIGHT, 8, 2, 0, 0, 0),
        "PNG header",
    )
    filtered = zlib.decompress(chunks[1][1])
    stride = CANONICAL_WIDTH * 3
    require_equal(len(filtered), (stride + 1) * CANONICAL_HEIGHT, "PNG data size")
    rows = []
    for row in range(CANONICAL_HEIGHT):
        start = row * (stride + 1)
        require_equal(filtered[start], 0, "PNG row filter")
        rows.append(filtered[start + 1 : start + 1 + stride])
    return b"".join(rows)


def static_manifest_fields() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "evidence_level": "single-independent-implementation",
        "suite": {
            "name": "Amiga Test Kit",
            "version": "1.21",
            "source_tag": "testkit-v1.21",
            "source_commit": "9477599d1611da2326f43532dbe563c2848e308b",
            "adf_sha256": ADF_SHA256,
        },
        "machine": {
            "model": "commodore-amiga-a1200-aga-pal",
            "cpu": "68EC020",
            "chipset": "AGA",
            "region": "PAL",
            "chip_ram_bytes": 2_097_152,
            "expansion_ram_bytes": 0,
            "kickstart_revision": "3.1 r40.068",
            "kickstart_sha256": FIRMWARE_SHA256,
        },
        "viewport": {
            "producer_raw_width": RAW_WIDTH,
            "producer_raw_height": RAW_HEIGHT,
            "producer_x": PRODUCER_CROP_X,
            "producer_y": PRODUCER_CROP_Y,
            "runtime_width": 768,
            "runtime_height": 576,
            "runtime_x": RUNTIME_CROP_X,
            "runtime_y": RUNTIME_CROP_Y,
            "width": CROP_WIDTH,
            "height": CROP_HEIGHT,
            "vertical_decimation": VERTICAL_DECIMATION,
            "canonical_width": CANONICAL_WIDTH,
            "canonical_height": CANONICAL_HEIGHT,
            "pixel_format": "rgb8",
            "alignment_search": False,
        },
        "comparison": {
            "format": "rgb8-exact",
            "channel_tolerance": 0,
            "reference_alpha": "discard-after-opaque-validation",
            "runtime_alpha": "must-be-opaque",
            "row_pair_policy": "require-identical-before-decimation",
        },
        "producer": {
            "id": "fs-uae-5.0.7-f362278c-a1200-aga-pal",
            "emulator": "FS-UAE",
            "version": "5.0.7",
            "revision": SOURCE_REVISION,
            "uae_base_version": "WinUAE 6.0.1",
            "implementation_family": "UAE",
            "configuration": "A1200 AGA PAL cycle-exact 68EC020",
            "capture_method": "environment-gated raw chipset framebuffer hook",
            "binary_sha256": BINARY_SHA256,
            "capture_patch_sha256": PATCH_SHA256,
        },
        "capture_adapter": CAPTURE_TOOLS,
        "execution": {
            "boot_fields": 600,
            "key_hold_fields": 3,
            "key_release_settle_fields": 1,
            "inter_key_fields": 50,
        },
    }


def package_runs(run_root: Path) -> None:
    frames: list[dict[str, Any]] = []
    expected_pngs: set[str] = set()
    for spec in CASE_SPECS:
        raw_frames, capture_summary = validate_run(run_root, spec)
        references = []
        for phase, file_name, raw_index in spec["references"]:
            rgb, alpha_values = canonical_rgb(raw_frames[raw_index])
            require_equal(alpha_values, [255], f"{spec['id']} reference alpha")
            png = encode_png(rgb)
            (PACKAGE_DIR / file_name).write_bytes(png)
            expected_pngs.add(file_name)
            references.append(
                {
                    "phase": phase,
                    "file": file_name,
                    "png_sha256": sha256_bytes(png),
                    "rgb_sha256": sha256_bytes(rgb),
                    "source_core_field": (
                        spec["first_capture_field"] + raw_index
                    ),
                    "source_raw_sha256": capture_summary["raw_sha256"][raw_index],
                }
            )
        frames.append(
            {
                "id": spec["id"],
                "navigation": spec["navigation"],
                "execution_settle_fields": spec["settle_fields"],
                "behaviour": spec["behaviour"],
                "capture_provenance": capture_summary,
                "references": references,
            }
        )

    existing_pngs = {path.name for path in PACKAGE_DIR.glob("*.png")}
    require_equal(existing_pngs, expected_pngs, "packaged PNG set")
    manifest = {
        **static_manifest_fields(),
        "frames": frames,
        "packaging": {
            "tool": "package.py",
            "tool_sha256": sha256_file(Path(__file__)),
            "python_version": sys.version.split()[0],
            "zlib_version": zlib.ZLIB_VERSION,
            "png_encoding": "RGB8 filter-none zlib-level-9",
        },
    }
    (PACKAGE_DIR / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    verify_package()


def verify_package() -> None:
    manifest = read_json(PACKAGE_DIR / "manifest.json")
    for name, expected in static_manifest_fields().items():
        require_equal(manifest.get(name), expected, f"manifest {name}")
    require_equal(
        manifest["packaging"]["tool_sha256"],
        sha256_file(Path(__file__)),
        "packaging tool",
    )

    expected_pngs: set[str] = set()
    frames = manifest["frames"]
    require_equal(len(frames), len(CASE_SPECS), "manifest frame count")
    for frame, spec in zip(frames, CASE_SPECS, strict=True):
        require_equal(frame["id"], spec["id"], "frame identifier")
        require_equal(frame["navigation"], spec["navigation"], "navigation")
        require_equal(
            frame["execution_settle_fields"],
            spec["settle_fields"],
            "settle fields",
        )
        require_equal(frame["behaviour"], spec["behaviour"], "behaviour")
        require_equal(
            len(frame["references"]), len(spec["references"]), "reference count"
        )
        for reference, expected_reference in zip(
            frame["references"], spec["references"], strict=True
        ):
            phase, file_name, _ = expected_reference
            require_equal(reference["phase"], phase, "reference phase")
            require_equal(reference["file"], file_name, "reference file")
            path = PACKAGE_DIR / file_name
            rgb = decode_png(path)
            require_equal(sha256_file(path), reference["png_sha256"], "PNG hash")
            require_equal(sha256_bytes(rgb), reference["rgb_sha256"], "RGB hash")
            expected_pngs.add(file_name)

    require_equal(
        {path.name for path in PACKAGE_DIR.glob("*.png")},
        expected_pngs,
        "reference PNG set",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    package = subparsers.add_parser("package")
    package.add_argument("run_root", type=Path)
    subparsers.add_parser("verify")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "package":
            package_runs(args.run_root.resolve())
        else:
            verify_package()
    except (KeyError, OSError, ValueError, json.JSONDecodeError, zlib.error) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

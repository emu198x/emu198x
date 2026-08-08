#!/usr/bin/env python3
"""Verify private Amiga closure inputs without recording their local paths."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any


SCHEMA_VERSION = "1.0.0"
LANES = ("golden-matrix", "catalogue-ten")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
DISK_EXTENSIONS = {
    "adf",
    "d64",
    "d71",
    "d81",
    "dsk",
    "g64",
    "ipf",
    "nib",
    "vdk",
    "woz",
}
FIRMWARE_EXTENSIONS = {"bin", "rom"}


class VerificationError(ValueError):
    """One manifest or asset failed the closure identity contract."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json_object(path: Path) -> tuple[dict[str, Any], bytes]:
    try:
        raw = path.read_bytes()
        value = json.loads(raw)
    except (OSError, json.JSONDecodeError) as error:
        raise VerificationError("manifest is missing, unreadable, or invalid") from error
    if not isinstance(value, dict):
        raise VerificationError("manifest must be a JSON object")
    return value, raw


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def require_identity(value: Any, context: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{context}: identity must be an object")
    require(set(value) == {"bytes", "sha256"}, f"{context}: identity fields")
    require(
        isinstance(value["bytes"], int) and value["bytes"] >= 0,
        f"{context}: invalid byte count",
    )
    require(
        isinstance(value["sha256"], str)
        and SHA256_RE.fullmatch(value["sha256"]) is not None,
        f"{context}: invalid SHA-256",
    )
    return value


def require_relative_path(value: Any, context: str) -> str:
    require(isinstance(value, str) and value, f"{context}: missing relative path")
    path = PurePosixPath(value)
    require(not path.is_absolute(), f"{context}: path must be relative")
    require(".." not in path.parts, f"{context}: path must not traverse its root")
    return value


def validate_manifest(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    require(manifest.get("schema_version") == SCHEMA_VERSION, "unsupported schema")
    roots = manifest.get("roots")
    assets = manifest.get("assets")
    scope = manifest.get("scope")
    require(isinstance(roots, dict) and roots, "roots must be a non-empty object")
    require(isinstance(assets, list) and assets, "assets must be a non-empty array")
    require(isinstance(scope, dict), "scope must be an object")

    ids: set[str] = set()
    source_use_count = 0
    for asset in assets:
        require(isinstance(asset, dict), "asset entry must be an object")
        asset_id = asset.get("id")
        require(isinstance(asset_id, str) and asset_id, "asset ID is missing")
        require(asset_id not in ids, f"duplicate asset ID {asset_id}")
        ids.add(asset_id)
        kind = asset.get("kind")
        require(kind in {"disk", "firmware"}, f"{asset_id}: unsupported kind")
        require_identity(asset.get("payload"), f"{asset_id}: payload")
        uses = asset.get("uses")
        require(isinstance(uses, list) and uses, f"{asset_id}: uses must be non-empty")
        source_use_count += len(uses)

        for use in uses:
            require(isinstance(use, dict), f"{asset_id}: use must be an object")
            lane = use.get("lane")
            require(
                lane in {"golden-matrix", "catalogue-ten"},
                f"{asset_id}: unsupported lane",
            )
            root = use.get("root")
            require(root in roots, f"{asset_id}/{lane}: unknown root")
            consumers = use.get("consumers")
            require(
                isinstance(consumers, list)
                and consumers
                and all(isinstance(item, str) and item for item in consumers),
                f"{asset_id}/{lane}: consumers must be non-empty strings",
            )
            require_identity(use.get("source"), f"{asset_id}/{lane}: source")
            root_kind = roots[root].get("kind")
            require(root_kind in {"directory", "file"}, f"{root}: invalid kind")
            if root_kind == "directory":
                require_relative_path(
                    use.get("relative_path"), f"{asset_id}/{lane}"
                )
            else:
                require(
                    "relative_path" not in use,
                    f"{asset_id}/{lane}: file root must not have a relative path",
                )
            archive_member = use.get("archive_member")
            if archive_member is not None:
                require_relative_path(archive_member, f"{asset_id}/{lane}: member")

    require(
        scope.get("logical_asset_count") == len(assets),
        "scope logical_asset_count does not match assets",
    )
    require(
        scope.get("source_use_count") == source_use_count,
        "scope source_use_count does not match uses",
    )
    require(
        scope.get("lanes") == ["golden-matrix", "catalogue-ten"],
        "scope lanes are not the closure lane pair",
    )
    return assets


def resolve_source(use: dict[str, Any], roots: dict[str, Path]) -> Path:
    root_id = use["root"]
    try:
        root = roots[root_id]
    except KeyError as error:
        raise VerificationError(f"root {root_id} was not supplied") from error
    relative = use.get("relative_path")
    return root if relative is None else root / relative


def selected_archive_payload(
    source_path: Path,
    kind: str,
    expected_member: str,
    context: str,
) -> bytes:
    extensions = DISK_EXTENSIONS if kind == "disk" else FIRMWARE_EXTENSIONS
    try:
        with zipfile.ZipFile(source_path) as archive:
            matches = sorted(
                info.filename
                for info in archive.infolist()
                if not info.is_dir()
                and PurePosixPath(info.filename).suffix.removeprefix(".").lower()
                in extensions
            )
            require(matches, f"{context}: archive has no loadable {kind} member")
            if len(matches) == 1:
                selected = matches[0]
            else:
                roots = [name for name in matches if "/" not in name]
                require(
                    len(roots) == 1,
                    f"{context}: archive member selection is ambiguous",
                )
                selected = roots[0]
            require(
                selected == expected_member,
                f"{context}: selected archive member does not match manifest",
            )
            return archive.read(selected)
    except (OSError, zipfile.BadZipFile) as error:
        raise VerificationError(f"{context}: cannot read source archive") from error


def read_source(path: Path, context: str) -> bytes:
    try:
        return path.read_bytes()
    except OSError as error:
        raise VerificationError(f"{context}: source is missing or unreadable") from error


def verify_manifest(
    manifest_path: Path,
    roots: dict[str, Path],
    selected_lanes: tuple[str, ...] = LANES,
) -> dict[str, Any]:
    manifest, manifest_raw = read_json_object(manifest_path)
    assets = validate_manifest(manifest)
    require(bool(selected_lanes), "at least one lane must be selected")
    require(
        set(selected_lanes).issubset(LANES),
        "selected lanes are outside the closure asset scope",
    )
    source_use_count = 0
    verified_assets: list[dict[str, Any]] = []

    for asset in assets:
        asset_id = asset["id"]
        kind = asset["kind"]
        expected_payload = asset["payload"]
        selected_uses = [
            use for use in asset["uses"] if use["lane"] in selected_lanes
        ]
        if not selected_uses:
            continue
        for use in selected_uses:
            lane = use["lane"]
            context = f"{asset_id}/{lane}"
            source_path = resolve_source(use, roots)
            source = read_source(source_path, context)
            expected_source = use["source"]
            require(
                len(source) == expected_source["bytes"],
                f"{context}: source byte count mismatch",
            )
            require(
                sha256_bytes(source) == expected_source["sha256"],
                f"{context}: source SHA-256 mismatch",
            )

            member = use.get("archive_member")
            if member is None:
                require(
                    source_path.suffix.lower() != ".zip",
                    f"{context}: zip source lacks archive_member",
                )
                payload = source
            else:
                require(
                    source_path.suffix.lower() == ".zip",
                    f"{context}: archive_member requires a zip source",
                )
                payload = selected_archive_payload(
                    source_path, kind, member, context
                )
            require(
                len(payload) == expected_payload["bytes"],
                f"{context}: payload byte count mismatch",
            )
            require(
                sha256_bytes(payload) == expected_payload["sha256"],
                f"{context}: payload SHA-256 mismatch",
            )
            source_use_count += 1

        verified_assets.append(
            {
                "id": asset_id,
                "payload_bytes": expected_payload["bytes"],
                "payload_sha256": expected_payload["sha256"],
                "source_uses": len(selected_uses),
            }
        )

    return {
        "schema_version": SCHEMA_VERSION,
        "status": "pass",
        "manifest_sha256": sha256_bytes(manifest_raw),
        "lanes": [lane for lane in LANES if lane in selected_lanes],
        "logical_asset_count": len(verified_assets),
        "source_use_count": source_use_count,
        "assets": verified_assets,
    }


def optional_path(argument: Path | None, environment: str) -> Path | None:
    if argument is not None:
        return argument
    value = os.environ.get(environment)
    return Path(value) if value else None


def parse_args() -> argparse.Namespace:
    repo_root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=repo_root / "test-data/commodore/amiga/closure-assets-v1.json",
    )
    parser.add_argument("--golden-rom-root", type=Path)
    parser.add_argument("--golden-media-root", type=Path)
    parser.add_argument("--catalogue-firmware-root", type=Path)
    parser.add_argument("--catalogue-media-root", type=Path)
    parser.add_argument("--a1000-kickstart-disk", type=Path)
    parser.add_argument("--lane", action="append", choices=LANES)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    home = Path.home()
    catalogue_media = optional_path(
        args.catalogue_media_root, "EMU198X_CATALOGUE_MEDIA_ROOT"
    )
    a1000_disk = optional_path(
        args.a1000_kickstart_disk, "EMU198X_AMIGA_A1000_KICKSTART_DISK"
    )
    selected_lanes = tuple(lane for lane in LANES if lane in (args.lane or LANES))
    if "catalogue-ten" in selected_lanes and catalogue_media is None:
        print("error: EMU198X_CATALOGUE_MEDIA_ROOT is required", file=sys.stderr)
        return 2
    if "golden-matrix" in selected_lanes and a1000_disk is None:
        print("error: EMU198X_AMIGA_A1000_KICKSTART_DISK is required", file=sys.stderr)
        return 2

    roots = {
        "golden-rom": args.golden_rom_root
        or home / ".emu198x/roms/commodore-amiga",
        "golden-media": args.golden_media_root
        or home / ".emu198x/media/commodore-amiga",
        "catalogue-firmware": optional_path(
            args.catalogue_firmware_root, "EMU198X_CATALOGUE_FIRMWARE_ROOT"
        )
        or home / ".emu198x/roms",
    }
    if catalogue_media is not None:
        roots["catalogue-media"] = catalogue_media
    if a1000_disk is not None:
        roots["a1000-kickstart-disk"] = a1000_disk
    try:
        report = verify_manifest(args.manifest, roots, selected_lanes)
    except (OSError, json.JSONDecodeError, VerificationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Audit this captured ID-CRC study, not hardware or ROM qualification."""

from __future__ import annotations

import hashlib
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def crc(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ (0x1021 if value & 0x8000 else 0)) & 0xFFFF
    return value


def audit(record: dict, files: dict[str, bytes]) -> dict:
    assert record["qualified"] is False and record["origin"] == "fresh_execution"
    assert record["checks_passed"] is True and record["fuse_source"]["patched"] is False
    assert record["fuse_source"]["commit"] == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    assert (
        record["fuse_source"]["files"]["peripherals/disk/upd_fdc.c"]
        == "845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d"
    )
    for name, wanted in record["authored_sources"].items():
        assert sha((HERE / name).read_bytes()) == wanted
    wanted = {
        (repeat, 0x42, n, mask)
        for repeat in [1, 2]
        for mask in [0, 1, 2, 3]
        for n in [0, 1, 17, 512]
    }
    seen = set()
    pairs = {}
    clean = next(files[c["raw_path"]] for c in record["cases"] if c["crc_mask"] == 0)
    assert len(clean) == 8599 and clean[:3] == bytes([106, 24, 0])
    header = bytes([0xA1] * 3 + [0xFE, 0, 0, 1, 2])
    data = bytes([0xA1] * 3 + [0xFB] + [0x21] * 512)
    assert clean[3 + 92 : 3 + 100] == header
    assert clean[3 + 136 : 3 + 652] == data
    assert int.from_bytes(clean[3 + 100 : 3 + 102], "big") == crc(header)
    assert int.from_bytes(clean[3 + 652 : 3 + 654], "big") == crc(data)
    clocks = clean[6253:7035]
    assert {i for i in range(6250) if clocks[i >> 3] & (1 << (i & 7))} == {
        92,
        93,
        94,
        136,
        137,
        138,
    }
    for case in record["cases"]:
        repeat, op, n, mask = key = tuple(
            case[k] for k in ["repeat", "opcode", "consumed", "crc_mask"]
        )
        assert key in wanted and key not in seen
        seen.add(key)
        observed = case["observed"]
        assert (observed["opcode"], observed["consumed"], observed["crc_mask"]) == (
            op,
            n,
            mask,
        )
        assert observed["idle_msr"] == 128 and observed["media_unchanged"] is True
        if n == 512:
            expected = [
                0,
                0x24 if mask & 1 else 0x20 if mask & 2 else 0,
                0x20 if mask & 2 else 0,
                0,
                0,
                2,
                2,
            ]
        else:
            expected = [0x40, 0x34 if mask & 1 else 0x10, 0, 0, 0, 1, 2]
        assert observed["result"] == expected
        raw = files[case["raw_path"]]
        assert sha(raw) == case["raw_sha256"]
        altered = bytearray(clean)
        if mask & 1:
            altered[103] ^= 1
        if mask & 2:
            altered[655] ^= 1
        assert raw == altered
        pair = (op, n, mask)
        if repeat == 1:
            pairs[pair] = observed
        else:
            assert pairs[pair] == observed
    assert seen == wanted and len(record["commands"]) == 34
    for command in record["commands"]:
        assert command["returncode"] == 0
        assert command["elapsed_seconds"] < command["timeout_seconds"]
        assert not files[command["name"] + ".stderr"]
    return {
        "qualified": False,
        "observations": 32,
        "identical_repeat_pairs": 16,
        "independently_audited_raw_fields": 64,
        "checks_passed": True,
    }


def load_files() -> dict[str, bytes]:
    if (HERE / "runs").exists():
        return {
            p.name: p.read_bytes() for p in (HERE / "runs").iterdir() if p.is_file()
        }
    with tarfile.open(HERE.parent / "evidence" / "stock-fuse.tar.gz") as archive:
        files = {}
        for member in archive:
            assert member.isfile() and Path(member.name).name == member.name
            assert member.name not in files
            stream = archive.extractfile(member)
            assert stream is not None
            files[member.name] = stream.read()
        return files


def main() -> None:
    files = load_files()
    hashes = json.loads(files["hashes.json"])
    for name, wanted in hashes.items():
        assert sha(files[name]) == wanted
    print(json.dumps(audit(json.loads(files["campaign.json"]), files), indent=2))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Audit captured controller observations; no hardware qualification."""

import hashlib
import json
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def crc(data: bytes) -> int:
    value = 0xFFFF
    for byte in data:
        value ^= byte << 8
        for _ in range(8):
            value = ((value << 1) ^ (0x1021 if value & 0x8000 else 0)) & 0xFFFF
    return value


def audit() -> dict:
    global HERE
    if not (HERE / "runs").exists():
        previous = HERE
        with tempfile.TemporaryDirectory(prefix="198x-read-id-audit-") as directory:
            root = Path(directory)
            with tarfile.open(HERE.parent / "evidence/triage.tar.gz") as archive:
                seen = set()
                for member in archive:
                    name = Path(member.name)
                    assert (
                        member.isfile()
                        and not name.is_absolute()
                        and ".." not in name.parts
                        and member.name not in seen
                    )
                    seen.add(member.name)
                    stream = archive.extractfile(member)
                    assert stream is not None
                    target = root / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(stream.read())
            HERE = root
            try:
                return audit()
            finally:
                HERE = previous
    r = json.loads((HERE / "runs/campaign.json").read_text())
    assert r["qualified"] is False and r["checks_passed"] is True
    assert r["fuse_source"]["patched"] is False
    assert r["fuse_source"]["commit"] == "0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2"
    for name, wanted in r["authored_sources"].items():
        assert hashlib.sha256((HERE / name).read_bytes()).hexdigest() == wanted
    for name, wanted in json.loads((HERE / "runs/hashes.json").read_text()).items():
        assert hashlib.sha256((HERE / "runs" / name).read_bytes()).hexdigest() == wanted
    wanted = {
        (rep, op, mask) for rep in [1, 2] for op in [0x46, 0x4C] for mask in [0, 1, 2]
    }
    seen = set()
    pairs = {}
    for c in r["cases"]:
        key = (c["repeat"], c["opcode"], c["crc_mask"])
        assert key in wanted and key not in seen
        seen.add(key)
        op = c["opcode"]
        mask = c["crc_mask"]
        o = c["observed"]
        assert o["opcode"] == op and o["crc_mask"] == mask
        assert o["consumed"] == (0 if mask == 1 else 512)
        assert o["idle_msr"] == 128 and o["media_unchanged"] is True
        assert o["result"] == [
            64,
            5 if mask == 1 else 32 if mask == 2 else 128,
            32 if mask == 2 else 0,
            0,
            0,
            1,
            2,
        ]
        raw = (HERE / "runs" / c["raw_path"]).read_bytes()
        assert len(raw) == 8599
        assert hashlib.sha256(raw).hexdigest() == c["raw_sha256"]
        clean_case = next(
            x for x in r["cases"] if x["opcode"] == op and x["crc_mask"] == 0
        )
        clean = bytearray((HERE / "runs" / clean_case["raw_path"]).read_bytes())
        if mask == 1:
            clean[103] ^= 1
        if mask == 2:
            clean[655] ^= 1
        assert raw == clean
        track = raw[3:6253]
        header = bytes([0xA1] * 3 + [0xFE, 0, 0, 1, 2])
        data = bytes([0xA1] * 3 + [0xF8 if op == 0x4C else 0xFB] + [0x21] * 512)
        assert track[92:100] == header and track[136:652] == data
        assert int.from_bytes(track[100:102], "big") == crc(header) ^ (
            0x100 if mask == 1 else 0
        )
        assert int.from_bytes(track[652:654], "big") == crc(data) ^ (
            0x100 if mask == 2 else 0
        )
        clocks = raw[6253:7035]
        assert {i for i in range(6250) if clocks[i >> 3] & (1 << (i & 7))} == {
            92,
            93,
            94,
            136,
            137,
            138,
        }
        pair = (op, mask)
        if c["repeat"] == 1:
            pairs[pair] = o
        else:
            assert pairs[pair] == o
    assert seen == wanted and len(r["commands"]) == 14
    for command in r["commands"]:
        assert (
            command["returncode"] == 0
            and command["elapsed_seconds"] < command["timeout_seconds"]
        )
        assert not (HERE / "runs" / (command["name"] + ".stderr")).read_bytes()
    e = json.loads((HERE / "emu.json").read_text())
    assert (
        e["qualified"] is False and e["returncode"] == 0 and e["elapsed_seconds"] < 15
    )
    assert not (HERE / "emu-bounded.stderr").read_bytes()
    observations = [
        json.loads(line)
        for line in (HERE / "emu-bounded.stdout").read_text().splitlines()
    ]
    seen = set()
    for o in observations:
        key = (o["repeat"], o["opcode"], o["crc_mask"])
        assert key in wanted and key not in seen
        seen.add(key)
        assert (
            o["consumed"] == 512 and o["idle_msr"] == 128 and 0 < o["ticks"] < 1000000
        )
        assert o["result"] == [
            64,
            128 if o["crc_mask"] == 0 else 32,
            32 if o["crc_mask"] == 2 else 0,
            0,
            0,
            1,
            2,
        ]
    assert seen == wanted
    for name, wanted_hash in e["source_files"].items():
        assert (
            hashlib.sha256((HERE / "baseline-source" / name).read_bytes()).hexdigest()
            == wanted_hash
        )
    for name, wanted_hash in e["authored_files"].items():
        assert hashlib.sha256((HERE / name).read_bytes()).hexdigest() == wanted_hash
    return {
        "qualified": False,
        "checks_passed": True,
        "fuse_observations": 12,
        "emu_observations": 12,
        "independent_raw_crc_checks": 24,
    }


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2))

#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Audit byte/CRC/clock inputs and complete repeated observations, also under -O."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tarfile
import tempfile
from pathlib import Path

from cases import matrix

HERE = Path(__file__).resolve().parent


def require(ok: bool, reason: str) -> None:
    if not ok:
        raise ValueError(reason)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def raw_audit(raw: bytes, case: dict) -> dict:
    require(len(raw) == 8599 and raw[:3] == bytes([106, 24, 0]), "raw geometry")
    data, clocks = raw[3:6253], raw[6253:7035]
    require(raw[7035:] == bytes(1564), "unexpected FM/weak bitmap")
    expected_marks = []
    position = 80
    require(data[:position] == bytes([78]) * position, "initial gap")

    def consume_gap(length: int, byte: int) -> None:
        nonlocal position
        require(
            data[position : position + length] == bytes([byte]) * length, "gap bytes"
        )
        position += length

    def field(body: bytes | list[int]) -> None:
        nonlocal position
        expected_marks.extend(range(position, position + 3))
        message = bytes([161]) * 3 + bytes(body)
        require(
            data[position : position + len(message)] == message, "field contents/length"
        )
        # Independent bit-serial CRC, generator uses binascii.crc_hqx.
        crc = 65535
        for byte in message:
            crc ^= byte << 8
            for _ in range(8):
                crc = ((crc << 1) ^ 0x1021 if crc & 0x8000 else crc << 1) & 65535
        require(
            data[position + len(message) : position + len(message) + 2]
            == crc.to_bytes(2, "big"),
            "field CRC",
        )
        position += len(message) + 2

    for c, h, r, n, length, value in case["sectors"]:
        consume_gap(12, 0)
        field([254, c, h, r, n])
        consume_gap(22, 78)
        consume_gap(12, 0)
        field(bytes([251]) + bytes([value]) * length)
        consume_gap(42, 78)
    consume_gap(6250 - position, 78)
    marks = [i for i in range(6250) if clocks[i // 8] & (1 << (i % 8))]
    require(marks == expected_marks and clocks[-1] & 0xFC == 0, "raw clock bitmap")
    return {
        "id_fields": len(case["sectors"]),
        "data_fields": len(case["sectors"]),
        "crc_checks": 2 * len(case["sectors"]),
    }


def nominal(case: dict) -> bool:
    """Unambiguous equal-length fields only; not an oracle for boundaries."""
    return (
        case["present"]
        and bool(case["sectors"])
        and case["command"][6] > 0
        and case["command"][5] <= 5
        and case["command"][8] >= 128
        and all(
            s[3] == case["command"][5] and s[4] == 128 << s[3] for s in case["sectors"]
        )
    )


def audit(root: Path, expectations: dict | None = None) -> dict:
    hashes = json.loads((root / "hashes.json").read_text())
    require(bool(hashes), "empty hash manifest")
    require(
        set(hashes)
        == {p.name for p in root.iterdir() if p.is_file() and p.name != "hashes.json"},
        "missing/extra evidence file",
    )
    for name, want in hashes.items():
        require(Path(name).name == name, "unsafe evidence path")
        require(digest((root / name).read_bytes()) == want, "evidence hash " + name)
    campaign = json.loads((root / "campaign.json").read_text())
    with tarfile.open(root / "capture-source.tar.gz") as archive:
        for name, expected in campaign["recipe_sha256"].items():
            source = archive.extractfile(name)
            require(
                source is not None and digest(source.read()) == expected,
                "captured recipe identity",
            )
        for name, expected in (
            (
                "crates/nec-upd765a/tests/read_track_fields.rs",
                campaign["emu_source"]["probe_sha256"],
            ),
            (
                "crates/nec-upd765a/src/lib.rs",
                campaign["emu_source"]["controller_sha256"],
            ),
        ):
            source = archive.extractfile(name)
            require(
                source is not None and digest(source.read()) == expected,
                "captured Emu source identity",
            )
    cases = matrix()
    bycase = {c["name"]: c for c in cases}
    require(
        campaign["qualified"] is False and campaign["status"] == "captured",
        "scope or capture status",
    )
    require(campaign["private_copies_removed"] is True, "private firmware cleanup")
    # Ignore only the additional independently validated MFI audit summary.
    recorded = [
        {k: v for k, v in c.items() if k != "mfi_audit"} for c in campaign["cases"]
    ]
    require(recorded == cases, "changed frozen matrix")
    require(
        len(campaign["firmware_sha256"]) == 4
        and len(set(campaign["firmware_sha256"])) == 4,
        "firmware identities",
    )
    decoder_spec = importlib.util.spec_from_file_location(
        "field_decode", HERE.parent / "2026-10-19-read-track-raw-crc/decode.py"
    )
    require(
        decoder_spec is not None and decoder_spec.loader is not None, "decoder helper"
    )
    decoder = importlib.util.module_from_spec(decoder_spec)
    decoder_spec.loader.exec_module(decoder)
    raw_checks = {}
    for c in cases:
        fixture = (root / (c["name"] + ".txt")).read_text().splitlines()
        require(fixture[0] == c["name"], "wrong fixture case")
        require(
            list(map(int, fixture[1].split())) == c["command"], "wrong fixture command"
        )
        require(fixture[2] == str(int(c["present"])), "wrong media presence")
        require(
            [list(map(int, line.split())) for line in fixture[3:]] == c["sectors"],
            "wrong fixture sectors",
        )
        if c["raw"]:
            raw = (root / (c["name"] + ".raw")).read_bytes()
            raw_checks[c["name"]] = raw_audit(raw, c)
            checked = decoder.validate((root / (c["name"] + ".mfi")).read_bytes(), raw)
            require(
                checked
                == next(
                    x["mfi_audit"] for x in campaign["cases"] if x["name"] == c["name"]
                ),
                "MFI audit record",
            )
    rows = campaign["observations"]
    index = {}
    commands = {}
    for cmd in campaign["commands"]:
        require(cmd["name"] not in commands, "duplicate command")
        commands[cmd["name"]] = cmd
        require(cmd["host_timeout_seconds"] <= 180, "unbounded host timeout")
    for row in rows:
        key = (row["engine"], row["repeat"], row["case"])
        require(key not in index, "duplicate observation")
        index[key] = row
    wanted = {
        (e, r, c["name"])
        for e in ("emu", "fuse", "mame")
        for r in (1, 2)
        for c in cases
    }
    require(set(index) == wanted, "missing/extra observation")
    differences = []
    failed = []
    assertions = 0
    for key, row in index.items():
        engine, repeat, name = key
        c = bycase[name]
        if expectations is not None:
            require(
                row["status"] == expectations[f"{engine}:{name}"]["status"],
                "changed expected capture status",
            )
        require(row["evidence_class"] == "fresh_execution", "replay labelled fresh")
        if engine != "emu" and not c["raw"]:
            require(
                row["status"] == "not_applicable" and row.get("reason"),
                "untruthful non-applicability",
            )
            continue
        cmd = commands.get(row["name"])
        require(cmd is not None, "missing executed command")
        if row["status"] == "fail":
            require(row.get("reason"), "missing failure reason")
            failed.append(list(key))
            continue
        require(
            row["status"] == "pass" and cmd["returncode"] == 0 and not cmd["timed_out"],
            "invalid successful observation",
        )
        if engine == "fuse":
            require(
                not (root / (row["name"] + ".stderr")).read_bytes(),
                "reference sanitizer diagnostic",
            )
            require(
                list(map(int, cmd["argv"][-9:])) == c["command"], "wrong Fuse command"
            )
        elif engine == "mame":
            require(
                list(map(int, cmd["environment"]["PROBE_COMMAND"].split()))
                == c["command"],
                "wrong MAME command",
            )
        else:
            require(
                Path(cmd["environment"]["READ_TRACK_FIELDS"]).name == name + ".txt",
                "wrong Emu fixture",
            )
        obs = row["observed"]
        if engine in ("emu", "fuse"):
            log = (root / (row["name"] + ".stdout")).read_text()
            line = next((line for line in log.splitlines() if '{"data":' in line), None)
            require(line is not None, "missing byte observation in raw log")
            require(
                json.loads(line[line.index('{"data":') :]) == obs,
                "raw log/campaign disagreement",
            )
        require(len(obs["result"]) == 7, "truncated/excess result")
        require(
            len(obs["data"]) <= 65536
            and all(
                type(b) == int and 0 <= b <= 255 for b in obs["data"] + obs["result"]
            ),
            "invalid bytes",
        )
        require(
            obs["idle_msr"] == 128
            and obs["recovery_msr"] == 128
            and type(obs["recovery"]) == int,
            "lost recovery",
        )
        require(obs["media_unchanged"] is True, "mutated source media")
        require(
            obs.get("ticks", 0) < 2000000
            and obs.get("tstates", 0) < 3546900 * 2
            and obs.get("seconds", 0) < 2,
            "emulated deadline",
        )
        other = index[(engine, 3 - repeat, name)]
        require(other["status"] == row["status"], "nonrepeatable status")
        if other["status"] == "pass":
            for field in (
                "data",
                "result",
                "idle_msr",
                "recovery",
                "recovery_msr",
                "media_unchanged",
            ):
                require(
                    obs[field] == other["observed"][field], "nonrepeatable " + field
                )
        if nominal(c) and engine == "emu":
            expected = []
            for i in range(c["command"][6]):
                s = c["sectors"][i % len(c["sectors"])]
                expected.extend([s[5]] * s[4])
            require(
                obs["data"] == expected, "wrong expected nominal size/order/payload"
            )
            assertions += len(expected)
        if expectations is not None:
            exp = expectations[f"{engine}:{name}"]
            require(obs["result"] == exp["result"], "wrong expected result")
            require(
                len(obs["data"]) == exp["payload_bytes"]
                and digest(bytes(obs["data"])) == exp["payload_sha256"],
                "wrong expected execution payload/size",
            )
        assertions += 7 + 4
    for c in cases:
        er = index[("emu", 1, c["name"])]
        for e in ("fuse", "mame"):
            rr = index[(e, 1, c["name"])]
            if er["status"] == "pass" and rr["status"] == "pass":
                fields = [
                    f
                    for f in ("data", "result")
                    if er["observed"][f] != rr["observed"][f]
                ]
                if fields:
                    differences.append(
                        {"case": c["name"], "reference": e, "fields": fields}
                    )
    require(assertions > 0, "zero assertions")
    return {
        "schema_version": 1,
        "qualified": False,
        "case_count": len(cases),
        "observations": len(rows),
        "successful_observations": sum(r["status"] == "pass" for r in rows),
        "failures": failed,
        "not_applicable": sum(r["status"] == "not_applicable" for r in rows),
        "disagreements": differences,
        "byte_and_state_assertions": assertions,
        "raw_checks": raw_checks,
    }


def main() -> None:
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--runs", type=Path)
    args = parser.parse_args()
    expectations = (
        json.loads((HERE / "expectations.json").read_text())
        if (HERE / "expectations.json").exists()
        else None
    )
    if args.runs:
        result = audit(args.runs, expectations)
    else:
        with tempfile.TemporaryDirectory(prefix="198x-field-offline-") as tmp:
            with tarfile.open(HERE / "evidence.tar.gz") as archive:
                archive.extractall(tmp, filter="data")
            result = audit(Path(tmp), expectations)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()

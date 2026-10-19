#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Offline framing, input and completeness audit; no inferred hardware verdict."""

from __future__ import annotations

import importlib.util
import json
import tarfile
import tempfile
from pathlib import Path

from cases import input_text, matrix

HERE = Path(__file__).resolve().parent


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ValueError("missing audit helper")
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


PREVIOUS = load(
    "previous_field_audit",
    HERE.parent / "2026-10-19-read-track-field-boundaries/verify.py",
)
DECODE = load(
    "independent_mfi_decoder", HERE.parent / "2026-10-19-read-track-raw-crc/decode.py"
)
require = PREVIOUS.require
sha = PREVIOUS.digest


def audit(root: Path, expected: dict) -> dict:
    hashes = json.loads((root / "hashes.json").read_text())
    require(
        set(hashes)
        == {p.name for p in root.iterdir() if p.is_file() and p.name != "hashes.json"},
        "missing/extra artifact",
    )
    for name, want in hashes.items():
        require(Path(name).name == name, "unsafe evidence path")
        require(sha((root / name).read_bytes()) == want, "artifact hash " + name)
    d = json.loads((root / "campaign.json").read_text())
    require(
        d["qualified"] is False and d["status"] == "captured", "scope or capture state"
    )
    require(d["private_copies_removed"] is True, "private firmware remains")
    require(len(d["firmware_sha256"]) == 4, "firmware identities")
    cases = matrix()
    case_by = {c["name"]: c for c in cases}
    require(
        [{k: v for k, v in c.items() if k != "mfi_audit"} for c in d["cases"]] == cases,
        "frozen matrix changed",
    )
    with tarfile.open(root / "capture-source.tar.gz") as source:
        identities = {
            **d["recipe_sha256"],
            **d["helper_sources"],
            "crates/nec-upd765a/src/lib.rs": d["emu_source"]["controller_sha256"],
            "crates/nec-upd765a/tests/read_track_fields.rs": d["emu_source"][
                "probe_sha256"
            ],
        }
        for name, want in identities.items():
            f = source.extractfile(name)
            require(f is not None and sha(f.read()) == want, "capture source identity")
    rows = {}
    commands = {}
    for cmd in d["commands"]:
        require(cmd["name"] not in commands, "duplicate command")
        commands[cmd["name"]] = cmd
        require(0 < cmd["host_timeout_seconds"] <= 180, "unbounded host runtime")
    for row in d["observations"]:
        key = (row["engine"], row["repeat"], row["case"])
        require(key not in rows, "duplicate row")
        rows[key] = row
    wanted = {
        (e, r, c["name"])
        for e in ("emu", "fuse", "mame")
        for r in (1, 2)
        for c in cases
    }
    require(set(rows) == wanted, "missing/extra row")
    accepted = rejected = not_applicable = byte_assertions = 0
    for (engine, repeat, name), row in rows.items():
        c = case_by[name]
        require(row["evidence_class"] == "fresh_execution", "fresh/replay identity")
        if engine != "emu" and not c["raw"]:
            require(
                row["status"] == "not_applicable"
                and "no faithful raw-track equivalent" in row["reason"],
                "missing container track not equivalent to raw gap",
            )
            not_applicable += 1
            continue
        require(row["status"] == "pass", "capture failed; no qualification credit")
        command = commands.get(row["name"])
        require(command is not None, "missing executed command")
        require(
            command["returncode"] == 0 and not command["timed_out"],
            "unsuccessful process",
        )
        o = row["observed"]
        sent = o["command_bytes_sent"]
        result = o["result"]
        data = o["data"]
        require(
            all(type(b) == int and 0 <= b <= 255 for b in data + result)
            and len(data) <= 65536,
            "invalid bytes/bound",
        )
        if sent == 1:
            require(result == [128] and not data, "one-byte rejection framing")
            require(
                engine == "fuse" and c["command"][0] & 128, "unexpected early rejection"
            )
            rejected += 1
        else:
            require(
                sent == 9 and len(result) == 7,
                "accepted command needs nine command/seven result bytes",
            )
            accepted += 1
            nominal = []
            for s in c["sectors"]:
                nominal.extend([s[5]] * s[4])
            require(data == nominal, "physical order/payload size mismatch")
            byte_assertions += len(nominal)
        require(
            o["idle_msr"] == 128
            and o["recovery_msr"] == 128
            and type(o["recovery"]) == int
            and 0 <= o["recovery"] < 256,
            "lost idle/next-command recovery",
        )
        require(o["media_unchanged"] is True, "mutated medium")
        require(
            o.get("ticks", 0) < 2000000
            and o.get("tstates", 0) < 7093800
            and o.get("seconds", 0) < 2,
            "emulated limit",
        )
        if engine in ("emu", "fuse"):
            require(
                not (root / (row["name"] + ".stderr")).read_bytes() or engine == "emu",
                "reference sanitizer diagnostic",
            )
            text = (root / (row["name"] + ".stdout")).read_text()
            line = next((s for s in text.splitlines() if '{"data":' in s), None)
            require(
                line is not None and json.loads(line[line.index('{"data":') :]) == o,
                "raw log differs from derived observation",
            )
        if engine == "fuse":
            require(
                list(map(int, command["argv"][-9:])) == c["command"],
                "wrong reference command",
            )
        if engine == "mame":
            require(
                list(map(int, command["environment"]["PROBE_COMMAND"].split()))
                == c["command"],
                "wrong MAME command",
            )
        if engine == "emu":
            require(
                Path(command["environment"]["READ_TRACK_FIELDS"]).name == name + ".txt",
                "wrong Emu fixture",
            )
        frozen = expected[f"{engine}:{name}"]
        require(
            sent == frozen["command_bytes_sent"] and result == frozen["result"],
            "wrong expected framing/result",
        )
        require(
            len(data) == frozen["payload_bytes"]
            and sha(bytes(data)) == frozen["payload_sha256"],
            "wrong expected payload/size",
        )
        twin = rows[(engine, 3 - repeat, name)]
        require(twin["status"] == "pass", "repeat status")
        for field in (
            "data",
            "result",
            "command_bytes_sent",
            "idle_msr",
            "recovery",
            "recovery_msr",
            "media_unchanged",
        ):
            require(o[field] == twin["observed"][field], "not repeatable " + field)
        byte_assertions += len(result) + 5
    raw_checks = {}
    for c in cases:
        require(
            (root / (c["name"] + ".txt")).read_text() == input_text(c),
            "wrong sector fixture input",
        )
        if c["raw"]:
            raw = (root / (c["name"] + ".raw")).read_bytes()
            raw_checks[c["name"]] = PREVIOUS.raw_audit(raw, c)
            checked = DECODE.validate((root / (c["name"] + ".mfi")).read_bytes(), raw)
            require(
                checked
                == next(x["mfi_audit"] for x in d["cases"] if x["name"] == c["name"]),
                "raw clock audit record",
            )
    require(
        accepted > 0 and rejected > 0 and byte_assertions > 0,
        "zero accepted/rejected assertions",
    )
    require(not_applicable == 16, "missing-entry reference scope")
    return {
        "schema_version": 1,
        "qualified": False,
        "cases": len(cases),
        "rows": len(rows),
        "accepted_command_captures": accepted,
        "early_rejection_captures": rejected,
        "not_applicable": not_applicable,
        "byte_and_state_assertions": byte_assertions,
        "raw_checks": raw_checks,
        "meaning": "capture integrity and expected-observation agreement, not silicon qualification",
    }


def main() -> None:
    expected = json.loads((HERE / "expectations.json").read_text())
    with tempfile.TemporaryDirectory(prefix="198x-opcode-offline-") as tmp:
        with tarfile.open(HERE / "evidence.tar.gz") as a:
            a.extractall(tmp, filter="data")
        print(json.dumps(audit(Path(tmp), expected), indent=2))


if __name__ == "__main__":
    main()

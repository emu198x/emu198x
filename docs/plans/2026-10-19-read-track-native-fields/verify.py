#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Audit the finite native continuation evidence, including failure controls."""

from __future__ import annotations

import hashlib
import json
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent


def require(value: bool, reason: str) -> None:
    if not value:
        raise ValueError(reason)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fixtures() -> dict:
    # Independently frozen from #1880, not parsed from the Rust generator.
    one = [
        ("n0-dtl1", 0, 0, 1, 128),
        ("n0-dtl127", 0, 0, 127, 128),
        ("n5", 5, 5, 255, 4096),
        ("command0-recorded2", 0, 2, 255, 512),
        ("command2-recorded0", 2, 0, 255, 128),
        ("represented0", 2, 2, 255, 0),
        ("represented128", 2, 2, 255, 128),
        ("represented768", 2, 2, 255, 768),
    ]
    result = {
        name: {
            "command_n": n,
            "recorded_n": rn,
            "dtl": dtl,
            "lengths": [length],
            "ids": [1],
            "eot": 1,
            "missing": False,
        }
        for name, n, rn, dtl, length in one
    }
    for name, ids, eot in [
        ("reversed-eot2", [2, 1], 2),
        ("duplicate-eot2", [1, 1], 2),
        ("forward-eot3", [1, 2], 3),
        ("reversed-eot3", [2, 1], 3),
    ]:
        result[name] = {
            "command_n": 2,
            "recorded_n": 2,
            "dtl": 255,
            "lengths": [512, 512],
            "ids": ids,
            "eot": eot,
            "missing": False,
        }
    result["missing-entry"] = {
        "command_n": 2,
        "recorded_n": 2,
        "dtl": 255,
        "lengths": [512],
        "ids": [1],
        "eot": 1,
        "missing": True,
    }
    return result


def keys(case: str, fixture: dict) -> set:
    length = 0 if fixture["missing"] else fixture["eot"] * (128 << fixture["command_n"])
    size = 128 << fixture["command_n"]
    points = {(case, "result", length, p) for p in (0, 1, 3, 6)}
    if length:
        prefixes = {
            0,
            1,
            length // 2,
            length - 1,
            size - 1,
            size,
            size + 1,
            2 * size - 1,
            2 * size,
            2 * size + 1,
        }
        points |= {
            (case, phase, p, 0)
            for p in prefixes
            if p < length
            for phase in ("request-low", "request-high")
        }
    return points


def row_key(row: dict) -> tuple:
    return row["case"], row["phase"], row["payload_prefix"], row["result_prefix"]


def audit(root: Path) -> dict:
    hashes = json.loads((root / "hashes.json").read_text())
    require(bool(hashes), "zero artifact identities")
    require(
        set(hashes)
        == {
            str(p.relative_to(root))
            for p in root.rglob("*")
            if p.is_file() and p.name != "hashes.json"
        },
        "missing/extra artifact",
    )
    for name, want in hashes.items():
        require(
            not Path(name).is_absolute() and ".." not in Path(name).parts,
            "unsafe artifact path",
        )
        require(sha((root / name).read_bytes()) == want, "artifact hash " + name)
    d = json.loads((root / "campaign.json").read_text())
    require(
        d["qualified"] is False and d["status"] == "captured",
        "invalid qualification/capture state",
    )
    require(
        d["evidence_class"] == "fresh_execution"
        and d["executable"]["delivery"] == "source_build",
        "execution identity",
    )
    require(d["configuration"]["snapshot_version"] == 6, "unexpected native version")
    require(
        d["bounds"]
        == {
            "host_seconds_per_execution": 120,
            "ticks_per_byte_wait": 10000,
            "max_payload_bytes": 16384,
            "result_bytes": 7,
        },
        "changed execution bounds",
    )
    with tarfile.open(root / "capture-source.tar.gz") as a:
        for name, want in {**d["source"]["files"], **d["recipe_sha256"]}.items():
            f = a.extractfile(name)
            require(f is not None and sha(f.read()) == want, "captured source identity")
    commands = {}
    for cmd in d["commands"]:
        require(cmd["name"] not in commands, "duplicate command")
        commands[cmd["name"]] = cmd
        require(
            0 < cmd["host_timeout_seconds"] <= 300 and not cmd["timed_out"],
            "host timeout/bound",
        )
    require(
        len(d["executions"]) == 2 and {r["repeat"] for r in d["executions"]} == {1, 2},
        "missing/duplicate repeat",
    )
    models = fixtures()
    wanted = set().union(*(keys(name, f) for name, f in models.items()))
    require(len(wanted) == 180, "invalid frozen coverage")
    repeats = {}
    assertions = 0
    for execution in d["executions"]:
        repeat = execution["repeat"]
        command = commands[f"positive-{repeat}"]
        require(
            execution["status"] == "pass" and command["returncode"] == 0,
            "unsuccessful native run",
        )
        require(
            execution["summary"]
            == {
                "schema_version": 1,
                "qualified": False,
                "cases": 13,
                "checkpoints": 180,
            },
            "zero or incomplete summary",
        )
        require(
            command["environment"].get("READ_TRACK_NATIVE_WRONG_EXPECTED") is None,
            "negative option in positive run",
        )
        log = (root / f"positive-{repeat}.stdout").read_text()
        raw = [
            json.loads(line.split("READ_TRACK_NATIVE ", 1)[1])
            for line in log.splitlines()
            if "READ_TRACK_NATIVE " in line
        ]
        require(
            raw == execution["observations"],
            "raw log differs from derived observations",
        )
        rows = {}
        for row in raw:
            key = row_key(row)
            require(key not in rows, "duplicate checkpoint")
            rows[key] = row
        require(set(rows) == wanted, "missing/extra checkpoint")
        repeats[repeat] = rows
        for key, row in rows.items():
            case, phase, prefix, rprefix = key
            f = models[case]
            require(all(row[k] == v for k, v in f.items()), "wrong fixture identity")
            require(
                row["qualified"] is False and row["schema_version"] == 1,
                "row scope/schema",
            )
            length = 0 if f["missing"] else f["eot"] * (128 << f["command_n"])
            require(row["payload_total"] == length, "wrong expected payload size")
            require(
                len(row["observed_data"]) == length - prefix
                and len(row["observed_result"]) == 7 - rprefix,
                "truncated/excess continuation",
            )
            require(
                row["observed_data"] == row["expected_data"],
                "wrong expected continuation payload",
            )
            require(
                row["observed_result"] == row["expected_result"],
                "wrong expected result",
            )
            require(
                all(
                    type(b) == int and 0 <= b <= 255
                    for b in row["observed_data"] + row["observed_result"]
                ),
                "invalid observed bytes",
            )
            baseline = rows[(case, "result", length, 0)]["expected_result"]
            require(
                row["expected_result"] == baseline[rprefix:],
                "inconsistent result suffix",
            )
            if length:
                baseline_data = rows[(case, "request-low", 0, 0)]["expected_data"]
                require(
                    row["expected_data"] == baseline_data[prefix:],
                    "inconsistent baseline payload suffix",
                )
            require(
                row["idle_msr"] == 128
                and row["recovery_msr"] == 128
                and type(row["recovery"]) == int
                and 0 <= row["recovery"] < 256,
                "lost next-command recovery",
            )
            require(
                all(
                    row.get(flag) is True
                    for flag in (
                        "exact_checkpoint",
                        "final_state_equal",
                        "source_unchanged",
                        "live_unchanged",
                    )
                ),
                "failed state/media assertion",
            )
            directory = root / f"repeat-{repeat}"
            require(
                row["checkpoint"] == f"{case}-{prefix}-{rprefix}-{phase}.state",
                "wrong checkpoint identity",
            )
            require(
                row["source"] == case + ".edsk"
                and row["live"] == case + ".live.postcard",
                "wrong fixture artifact identity",
            )
            state = (directory / row["checkpoint"]).read_bytes()
            require(
                200000 < len(state) < 2000000 and state[0] == 6,
                "truncated/invalid native v6 checkpoint",
            )
            require(
                (directory / row["source"])
                .read_bytes()
                .startswith(b"EXTENDED CPC DSK File"),
                "missing authored EDSK source",
            )
            require(
                len((directory / row["live"]).read_bytes()) > 0,
                "missing live image bytes",
            )
            assertions += len(row["observed_data"]) + len(row["observed_result"]) + 6
    require(repeats[1] == repeats[2], "nonrepeatable observations")
    for name in models:
        for ext in (".edsk", ".live.postcard"):
            require(
                (root / "repeat-1" / (name + ext)).read_bytes()
                == (root / "repeat-2" / (name + ext)).read_bytes(),
                "nonrepeatable fixture",
            )
    for row in repeats[1].values():
        require(
            (root / "repeat-1" / row["checkpoint"]).read_bytes()
            == (root / "repeat-2" / row["checkpoint"]).read_bytes(),
            "nonrepeatable native checkpoint",
        )
    negative = commands["wrong-expected"]
    control = d["wrong_expected"]
    require(
        negative["returncode"] is not None and negative["returncode"] != 0,
        "wrong continuation unexpectedly passed",
    )
    require(
        negative["environment"].get("READ_TRACK_NATIVE_WRONG_EXPECTED") == "1",
        "missing deliberate negative option",
    )
    require(
        control["status"] == "pass"
        and control["process_returncode"] == negative["returncode"],
        "negative control identity",
    )
    text = (root / "wrong-expected.stdout").read_bytes() + (
        root / "wrong-expected.stderr"
    ).read_bytes()
    require(
        b"restored continuation differs from expected payload" in text,
        "negative failed for wrong reason",
    )
    require(assertions > 0, "zero continuation assertions")
    counts = {name: len(keys(name, f)) for name, f in models.items()}
    return {
        "schema_version": 1,
        "qualified": False,
        "fixtures": 13,
        "repeats": 2,
        "checkpoints_per_repeat": 180,
        "fresh_native_restores": 360,
        "byte_and_state_assertions": assertions,
        "wrong_expected": "intended rejection observed",
        "fixture_checkpoints": counts,
        "reference_comparison": "not applicable to private native v6 state; earlier controller comparisons remain separate",
        "guest_loading": "not run in this state-preservation packet",
    }


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="198x-native-offline-") as temp:
        with tarfile.open(HERE / "evidence.tar.gz") as a:
            a.extractall(temp, filter="data")
        print(json.dumps(audit(Path(temp)), indent=2))


if __name__ == "__main__":
    main()

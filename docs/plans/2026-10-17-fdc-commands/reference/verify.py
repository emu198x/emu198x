# SPDX-License-Identifier: GPL-2.0-or-later
"""Verify retained observations and preserve known-reference failures truthfully."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
MAME = "6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9"
FIRMWARE = [
    "841d54545c743a7202fb9b068534ed906a08cc657a19955ae20a8de5af8ab185",
    "6d6ccab28f255b01c8cd89ceab9e7b60247f4610471118092f290f6c7768b7e3",
    "c5590212a269117657ce611acbac582e41e9db87dc78db480da2615e413b2da6",
    "8ec5cb9fd7ec24a041800314bb5a2c99be02f7e50745194eb649ed6d9085bd1f",
]


def require(ok: bool, reason: str) -> None:
    if not ok:
        raise ValueError(reason)


def check(record: dict) -> dict:
    require(
        record["kind"] == "direct-controller-reference-not-rom-qualification",
        "false route",
    )
    require(record["evidence_class"] == "fresh_execution", "wrong origin")
    require(record["mame"]["sha256"] == MAME, "wrong reference binary")
    require(record["firmware_sha256"] == FIRMWARE, "wrong firmware")
    require(len(record["cases"]) == 16, "incomplete cases")
    require(len({r["name"] for r in record["cases"]}) == 16, "duplicate cases")
    results = []
    for row in record["cases"]:
        require(
            row["exit"] == 0 and "observation" in row, "missing executed observation"
        )
        obs = row["observation"]
        require(0 < obs["seconds"] <= 2, "unbounded emulated time")
        actual = obs["observed"]
        require(
            actual["idle_msr"] == 0x80 and len(actual["result"]) == 7,
            "undrained result",
        )
        name = row["name"]
        if name == "write-deleted":
            require(
                actual["sent"] == 512
                and actual["result"] == [0x40, 0x80, 0, 0, 0, 1, 2],
                "wrong write reference",
            )
            readback = obs["readback"]
            require(
                readback["data"] == [0xA5] * 512
                and readback["result"] == [0x40, 0x80, 0, 0, 0, 1, 2],
                "deleted payload/mark missing",
            )
            results.append(
                {
                    "case": name,
                    "payload_and_mark": "pass",
                    "no_tc_write_chrn": "not_run",
                    "reason": "MAME C0 versus Emu/Fuse next-C1 remains unresolved; not promoted",
                }
            )
        elif name.startswith("track-"):
            ids = row["ids"][: row["eot"]]
            require(
                actual["data"] == [b for r in ids for b in [0x20 + r] * 512],
                "wrong physical sector order",
            )
            require(
                actual["result"]
                == [0x40, 0x80 | (4 if ids[-1] != row["first"] else 0), 0, 0, 0, 1, 2],
                "wrong track result",
            )
            results.append(
                {"case": name, "represented_payload_order_and_count": "pass"}
            )
        else:
            # Desired NEC/Fuse comparison flags are independent of MAME's
            # observed defects. Keep fail, never turn exit zero into conformance.
            if name == "scan-step":
                expected = [0, 0, 8, 0, 0, 3, 2]
                sent = 1024
            elif name == "scan-early-hit":
                expected = [0, 0, 8, 0, 0, 1, 2]
                sent = 512
            elif name == "scan-deleted":
                expected = [0, 0, 0x48, 0, 0, 1, 2]
                sent = 512
            elif name == "scan-mixed-low":
                expected = [0x40, 0x80, 4, 0, 0, 1, 2]
                sent = 512
            else:
                op, host = row["opcode"], row["host"]
                hit = (
                    host == 0x21
                    if op == 0x51
                    else 0x21 <= host
                    if op == 0x59
                    else 0x21 >= host
                )
                flags = 8 if host == 0x21 else 0 if hit else 4
                expected = [0 if hit else 0x40, 0 if hit else 0x80, flags, 0, 0, 1, 2]
                sent = 512
            results.append(
                {
                    "case": name,
                    "status": "pass"
                    if actual["result"] == expected and actual["sent"] == sent
                    else "fail",
                    "expected_result": expected,
                    "observed_result": actual["result"],
                    "expected_host_bytes": sent,
                    "observed_host_bytes": actual["sent"],
                }
            )
    return {
        "qualified": False,
        "scope": "controller reference observations; Scan disagreements retained",
        "cases": results,
    }


def main() -> dict:
    records = [json.loads((HERE / f"final-{i}.json").read_text()) for i in (1, 2)]
    for record in records:
        for name, digest in record["sources"].items():
            require(
                hashlib.sha256((HERE / name).read_bytes()).hexdigest() == digest,
                "changed executed source",
            )
        check(record)
    require(
        [r["observation"] for r in records[0]["cases"]]
        == [r["observation"] for r in records[1]["cases"]],
        "nonrepeatable observations",
    )
    return {**check(records[0]), "fresh_repeats": 2}


if __name__ == "__main__":
    print(json.dumps(main(), indent=2))

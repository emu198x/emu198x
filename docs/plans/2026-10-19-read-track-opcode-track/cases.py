# SPDX-License-Identifier: GPL-2.0-or-later
"""Finite opcode contrasts; missing container entry is not invented raw flux."""

from __future__ import annotations

import importlib.util
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRIOR = HERE.parent / "2026-10-19-read-track-field-boundaries"


def load_previous():
    spec = importlib.util.spec_from_file_location(
        "previous_fields_cases", PRIOR / "cases.py"
    )
    if spec is None or spec.loader is None:
        raise ValueError("missing preceding fixture")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def matrix() -> list[dict]:
    result = []
    for kind in ("ids-12", "ids-21", "empty-track", "missing-track"):
        for opcode in (0x42, 0x62, 0xC2, 0xE2):
            ids = [1, 2] if kind == "ids-12" else [2, 1] if kind == "ids-21" else []
            result.append(
                {
                    "name": f"{kind}-op-{opcode:02x}",
                    "command": [opcode, 0, 0, 0, 1, 2, 2 if ids else 1, 42, 255],
                    "sectors": [[0, 0, r, 2, 512, 33 + i] for i, r in enumerate(ids)],
                    "present": True,
                    "track_present": kind != "missing-track",
                    "raw": kind != "missing-track",
                    "safety_only": opcode != 0x42,
                    "scope": "opcode contrast; MT/SK-bit contrasts do not qualify multi-track or skip operation"
                    if opcode != 0x42
                    else "bounded sector-model observation",
                }
            )
    return result


def raw_track(case: dict) -> bytes:
    return load_previous().raw_track(case)


def input_text(case: dict) -> str:
    return (
        "\n".join(
            [
                case["name"],
                " ".join(map(str, case["command"])),
                str(1 if case["track_present"] else 2),
                *[" ".join(map(str, s)) for s in case["sectors"]],
            ]
        )
        + "\n"
    )

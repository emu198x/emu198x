"""Separate concrete-board evidence while retaining family shipping identities."""

from __future__ import annotations

from collections import defaultdict


def partitions(
    systems: list[dict[str, str]], closures: dict[str, set[str]]
) -> dict[str, tuple[list[str], list[str], list[str]]]:
    """Return own/shared crates and other machines sharing the binary.

    Older rows still use their shipping crate as the evidence root. Explicit
    machine_crate rows own only that concrete board's exclusive dependencies;
    the family host/runtime and genuinely common wiring remain shared.
    """
    reach: dict[str, set[str]] = defaultdict(set)
    for binary in {s["crate"] for s in systems}:
        for member in closures[binary]:
            reach[member].add(binary)
    board_rows = [s for s in systems if "machine_crate" in s]
    roots = [s["machine_crate"] for s in board_rows]
    if len(roots) != len(set(roots)):
        raise ValueError("two machine identities name the same concrete machine crate")
    board_reach: dict[str, set[str]] = defaultdict(set)
    for row in board_rows:
        root = row["machine_crate"]
        if not root.startswith("machine-") or root not in closures:
            raise ValueError(f"unknown concrete machine crate: {root}")
        if root not in closures[row["crate"]]:
            raise ValueError(f"{root} is not reachable from {row['crate']}")
        for member in closures[root]:
            board_reach[member].add(root)
    result = {}
    for row in systems:
        binary = row["crate"]
        cohabiting = sorted(
            s["machine_id"]
            for s in systems
            if s["crate"] == binary and s["machine_id"] != row["machine_id"]
        )
        if "machine_crate" in row:
            root = row["machine_crate"]
            own = {
                c
                for c in closures[root]
                if reach[c] == {binary} and board_reach[c] == {root}
            }
            other_only: set[str] = set()
            for other in board_rows:
                if other["crate"] == binary and other["machine_crate"] != root:
                    other_only |= closures[other["machine_crate"]] - closures[root]
            shared = closures[binary] - own - other_only
        else:
            own = {c for c in closures[binary] if reach[c] == {binary}}
            shared = {c for c in closures[binary] if len(reach[c]) > 1}
        result[row["machine_id"]] = (sorted(own), sorted(shared), cohabiting)
    return result

"""Retained cold-ROM regression auditor, adapted from the Read Data CRC study."""

from __future__ import annotations

import hashlib
import json
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE = "2ecf2a8eee85ecd94f45ea3089818afb9649b826"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def unpack(path: Path) -> dict[str, bytes]:
    with tarfile.open(path) as archive:
        result = {}
        for member in archive.getmembers():
            p = Path(member.name)
            require(
                member.isfile() and (not p.is_absolute()) and (".." not in p.parts),
                "runtime evidence invariant: member.isfile() and (not p.is_absolute()) and ('..' not in p.parts)",
            )
            require(
                member.name not in result,
                "runtime evidence invariant: member.name not in result",
            )
            stream = archive.extractfile(member)
            require(
                stream is not None, "runtime evidence invariant: stream is not None"
            )
            result[member.name] = stream.read()
        require(result, "runtime evidence invariant: result")
        return result


def audit_runtime(files: dict[str, bytes]) -> None:
    checks = json.loads(files["checks.json"])
    require(
        [c["exit"] for c in checks] == [0, 0, 1],
        "runtime evidence invariant: [c['exit'] for c in checks] == [0, 0, 1]",
    )
    require(
        [c["name"] for c in checks] == ["positive-1", "positive-2", "wrong-signature"],
        "runtime evidence invariant: [c['name'] for c in checks] == ['positive-1', 'positive-2', 'wrong-signature']",
    )
    for name in ["positive-1", "positive-2", "wrong-signature"]:
        r = json.loads(files[name + "/regression.json"])
        positive = name != "wrong-signature"
        require(
            r["qualified"] is False and r["regression_passed"] is positive,
            "runtime evidence invariant: r['qualified'] is False and r['regression_passed'] is positive",
        )
        require(
            r["fixture"]["origin"] == "captured_reference_replay",
            "runtime evidence invariant: r['fixture']['origin'] == 'captured_reference_replay'",
        )
        require(
            r["evidence_class"] == "fresh_execution"
            and r["delivery"] == "source_build",
            "runtime evidence invariant: r['evidence_class'] == 'fresh_execution' and r['delivery'] == 'source_build'",
        )
        require(
            r["repositories"]["emu"]["commit"] == SOURCE,
            "runtime evidence invariant: r['repositories']['emu']['commit'] == SOURCE",
        )
        require(
            r["repositories"]["emu"]["tracked_status"] == "",
            "runtime evidence invariant: r['repositories']['emu']['tracked_status'] == ''",
        )
        require(
            r["tools"]["emu"]["sha256"]
            == "c9a5dd2628450d81c8a2b21a46775ea3b6b4a2b533fc952816cf5bdf33d2606f",
            "runtime evidence invariant: r['tools']['emu']['sha256'] == 'c9a5dd2628450d81c8a2b21a46775ea3b6b4a2b533fc952816cf5bdf33d2606f'",
        )
        require(
            r["configuration"]["id"]
            == "spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1",
            "runtime evidence invariant: r['configuration']['id'] == 'spectrum-plus3-pal-40077-en40-rom-841d5454-6d6ccab2-c5590212-8ec5cb9f-drive-a-dsk180-v1'",
        )
        artifact_names = {artifact["path"] for artifact in r["artifacts"]}
        require(
            len(artifact_names) == len(r["artifacts"]),
            "runtime evidence invariant: len(artifact_names) == len(r['artifacts'])",
        )
        require(
            {
                "runtime.stdout",
                "runtime.stderr",
                "session.json",
                "diagnostic.bin",
                "diagnostic.dsk",
                "screen.png",
            }
            <= artifact_names,
            "runtime evidence invariant: {'runtime.stdout', 'runtime.stderr', 'session.json', 'diagnostic.bin', 'diagnostic.dsk', 'screen.png'} <= artifact_names",
        )
        for artifact in r["artifacts"]:
            data = files[name + "/" + artifact["path"]]
            require(
                len(data) == artifact["size"] and sha(data) == artifact["sha256"],
                "runtime evidence invariant: len(data) == artifact['size'] and sha(data) == artifact['sha256']",
            )
        for stage, count in [("native_format", 519), ("firmware", 8), ("loading", 3)]:
            require(
                r["stages"][stage]["status"] == "pass",
                "runtime evidence invariant: r['stages'][stage]['status'] == 'pass'",
            )
            require(
                r["stages"][stage]["assertions"] == count,
                "runtime evidence invariant: r['stages'][stage]['assertions'] == count",
            )
        require(
            all(
                r["stages"][s]["status"] == "not_run" for s in ["assembly", "mastering"]
            ),
            "runtime evidence invariant: all((r['stages'][s]['status'] == 'not_run' for s in ['assembly', 'mastering']))",
        )
        execution = r["stages"]["execution"]
        if positive:
            require(
                execution["status"] == "pass" and execution["assertions"] == 7,
                "runtime evidence invariant: execution['status'] == 'pass' and execution['assertions'] == 7",
            )
            observed = execution["observed"]
            require(
                observed["signature_before"] == [0, 0],
                "runtime evidence invariant: observed['signature_before'] == [0, 0]",
            )
            require(
                observed["signature_after"] == [90, 91],
                "runtime evidence invariant: observed['signature_after'] == [90, 91]",
            )
            require(
                observed["screen_sha256"]
                == "d2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2",
                "runtime evidence invariant: observed['screen_sha256'] == 'd2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2'",
            )
            require(
                observed["writes"][-2:]
                == [
                    {"pc": 32816, "addr": 28672, "value": 90, "size_bytes": 1},
                    {"pc": 32820, "addr": 28673, "value": 91, "size_bytes": 1},
                ],
                "runtime evidence invariant: observed['writes'][-2:] == [{'pc': 32816, 'addr': 28672, 'value': 90, 'size_bytes': 1}, {'pc': 32820, 'addr': 28673, 'value': 91, 'size_bytes': 1}]",
            )
        else:
            require(
                execution["status"] == "fail",
                "runtime evidence invariant: execution['status'] == 'fail'",
            )
            require(
                execution["reason"]
                == "signature mismatch: expected [90, 92], observed [90, 91]",
                "runtime evidence invariant: execution['reason'] == 'signature mismatch: expected [90, 92], observed [90, 91]'",
            )
        runtime = [c for c in r["commands"] if c["name"] == "runtime"]
        require(
            len(runtime) == 1 and runtime[0]["returncode"] == 0,
            "runtime evidence invariant: len(runtime) == 1 and runtime[0]['returncode'] == 0",
        )


def main() -> None:
    audit_runtime(unpack(HERE / "runtime-rom.tar.gz"))
    print("Fresh source ROM regression: two passes; wrong-signature control rejected")


if __name__ == "__main__":
    main()

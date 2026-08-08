#!/usr/bin/env python3
"""Run the revision-stamped Amiga accuracy-closure evidence lanes."""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from typing import Callable, Iterable, Mapping, Sequence


SCHEMA = "org.198x.emu198x.amiga-closure-report.v1"
REPORT_FILENAME = "report.json"
EVIDENCE_SCOPE = (
    "This report records the selected verification lanes at one Git revision. "
    "It is not a general Amiga-accuracy or physical-hardware-conformance claim."
)

ALLOWED_DISAGREEMENT_CLASSIFICATIONS = {
    "fixed",
    "scoped-out",
    "blocked-stronger-evidence",
}

EXPECTED_DISAGREEMENT_IDS = (
    "paula-stereo-channel-assignment",
    "lisa-color-output-delay",
    "denise-ocs-color-output-phase",
    "aga-sprite-horizontal-output-phase",
    "a1000-workbench-pointer-golden-baseline",
    "a1000-workbench-free-memory-readout",
    "disk-read-dma-request-stage",
    "programmable-hblank-ecsena-gate",
    "programmable-hblank-extblken-gate",
    "programmable-hblank-aga-blanken-path",
    "programmable-hblank-aga-fine-phase",
    "programmable-hblank-midline-write-timing",
    "paula-cross-producer-raw-waveform",
)

EXPECTED_CATALOGUE_IDS = (
    "workbench-1.3-desktop",
    "workbench-2.04-desktop",
    "barbarian",
    "1943",
    "arkanoid-revenge-of-doh",
    "bad-dudes-ecs",
    "workbench-3.1-desktop",
    "banshee-demo-aga",
    "state-of-the-art",
    "alien-syndrome-ntsc",
)

EXPECTED_TEST_KIT_V121_MARKERS = {
    "test-kit-v1.21-ocs": (
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: gradients matched "
        "registered disagreement signature(s): denise-ocs-color-output-phase",
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: static-checkerboard "
        "matched exactly",
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: alternating-checkerboard "
        "matched exactly",
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: ebu-bars matched "
        "registered disagreement signature(s): denise-ocs-color-output-phase",
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: dots matched exactly",
        "Amiga Test Kit v1.21 A500+A501 OCS PAL video: crosshatch matched exactly",
    ),
    "test-kit-v1.21-aga": (
        "Amiga Test Kit v1.21 A1200 AGA PAL video: gradients matched registered "
        "disagreement signature(s): aga-sprite-horizontal-output-phase",
        "Amiga Test Kit v1.21 A1200 AGA PAL video: static-checkerboard matched "
        "registered disagreement signature(s): aga-sprite-horizontal-output-phase",
        "Amiga Test Kit v1.21 A1200 AGA PAL video: alternating-checkerboard matched "
        "registered disagreement signature(s): aga-sprite-horizontal-output-phase",
        "Amiga Test Kit v1.21 A1200 AGA PAL video: ebu-bars matched exactly",
        "Amiga Test Kit v1.21 A1200 AGA PAL video: dots matched exactly",
        "Amiga Test Kit v1.21 A1200 AGA PAL video: crosshatch matched exactly",
    ),
}

# This registry is deliberately data rather than prose hidden in the runner.
# Each row is traceable to the current campaign/process documents and is copied
# unchanged into every report so a green command cannot erase evidence limits.
DISAGREEMENT_REGISTRY = [
    {
        "id": "paula-stereo-channel-assignment",
        "kind": "comparator-disagreement",
        "classification": "fixed",
        "summary": (
            "Emu198x's reversed channel assignment was corrected to channels "
            "1/2 left and 0/3 right after the hardware manual adjudicated the "
            "vAmiga disagreement."
        ),
        "documents": [
            "knowledge/decisions/amiga-paula-stereo-routing.md",
            "knowledge/processes/amiga-paula-audio-conformance.md",
        ],
    },
    {
        "id": "lisa-color-output-delay",
        "kind": "comparator-disagreement",
        "classification": "fixed",
        "summary": (
            "Lisa's one-hires-sample delay aligned the registered A1200 Test "
            "Kit COLOR transition boundaries. EBU bars are exact under the "
            "beam-absolute crop; gradients retain only the separately tracked "
            "pointer disagreement."
        ),
        "documents": [
            "knowledge/decisions/amiga-lisa-color-output-delay.md",
            "knowledge/processes/amiga-test-kit-video-conformance.md",
        ],
    },
    {
        "id": "denise-ocs-color-output-phase",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The A500 gradients and EBU bars retain exact registered "
            "disagreement signatures: vAmiga applies OCS Copper COLOR writes "
            "without the early stage observed in the UAE ECS/AGA family. "
            "Physical OCS evidence or another independent family is required."
        ),
        "documents": [
            "test-data/amiga-test-kit-v1.21/a500-a501-ocs-pal/assertions.json",
            "knowledge/decisions/amiga-denise-color-output-phase.md",
            "knowledge/processes/amiga-test-kit-video-conformance.md",
        ],
    },
    {
        "id": "aga-sprite-horizontal-output-phase",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The A1200 Test Kit retains an exact pointer-local disagreement "
            "footprint consistent with a two-host-sample displacement under "
            "the beam-absolute crop. Audited UAE source does not support "
            "adding a Lisa-only start delay; the machine-neutral sprite probe "
            "remains the adjudication path."
        ),
        "documents": [
            "test-data/amiga-test-kit-v1.21/a1200-aga-pal/assertions.json",
            "knowledge/decisions/amiga-sprite-horizontal-output-phase.md",
            "knowledge/processes/amiga-test-kit-video-conformance.md",
        ],
    },
    {
        "id": "a1000-workbench-pointer-golden-baseline",
        "kind": "regression-baseline-disagreement",
        "classification": "fixed",
        "summary": (
            "The stale A1000 Workbench 1.2 golden omitted the current desktop "
            "pointer. The reviewed baseline was replaced; pointer pixels remain "
            "unmasked and exact under the BPL1DAT sprite-visibility rule."
        ),
        "documents": [
            "crates/runtime-commodore-amiga/tests/golden_matrix.rs",
            "knowledge/decisions/amiga-denise-bpl1dat-sprite-visibility.md",
            "knowledge/processes/golden-image-capture.md",
        ],
    },
    {
        "id": "a1000-workbench-free-memory-readout",
        "kind": "assertion-boundary",
        "classification": "scoped-out",
        "summary": (
            "Only the six allocator-derived Workbench 1.2 free-memory digits "
            "are excluded by the exact 60x18 mask. Reviewed captures vary in "
            "16-byte allocator quanta; one comparison changed from 131288 to "
            "131224 bytes, or four quanta. The desktop pointer is outside the "
            "mask."
        ),
        "documents": [
            "crates/runtime-commodore-amiga/tests/golden_matrix.rs",
            "knowledge/decisions/amiga-denise-bpl1dat-sprite-visibility.md",
            "knowledge/processes/golden-image-capture.md",
        ],
    },
    {
        "id": "disk-read-dma-request-stage",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The D0/D1/D2 read request mask follows WinUAE's staged mapping; "
            "vAmiga takes the earliest available cell, so exact hardware stage "
            "selection still needs stronger evidence."
        ),
        "documents": [
            "knowledge/decisions/amiga-disk-dma-fifo-arbitration.md",
            "knowledge/decisions/amiga-accuracy-closure-campaign.md",
        ],
    },
    {
        "id": "programmable-hblank-ecsena-gate",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The audited UAE and Copperline families disagree on the ECSENA "
            "gate observation; the case remains measurement-only."
        ),
        "documents": [
            "knowledge/processes/amiga-programmable-hblank-conformance.md",
        ],
    },
    {
        "id": "programmable-hblank-extblken-gate",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The audited UAE and Copperline families disagree on the EXTBLKEN "
            "gate observation; the case remains measurement-only."
        ),
        "documents": [
            "knowledge/processes/amiga-programmable-hblank-conformance.md",
        ],
    },
    {
        "id": "programmable-hblank-aga-blanken-path",
        "kind": "comparator-disagreement",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The audited UAE and Copperline families disagree on AGA BLANKEN; "
            "only the ECS BLANKEN-clear observation is asserted."
        ),
        "documents": [
            "knowledge/processes/amiga-programmable-hblank-conformance.md",
        ],
    },
    {
        "id": "programmable-hblank-aga-fine-phase",
        "kind": "assertion-boundary",
        "classification": "scoped-out",
        "summary": (
            "The current portable comparison collapses horizontally duplicated "
            "pairs and does not claim the final AGA 35 ns phase bit."
        ),
        "documents": [
            "knowledge/processes/amiga-programmable-hblank-conformance.md",
        ],
    },
    {
        "id": "programmable-hblank-midline-write-timing",
        "kind": "evidence-gap",
        "classification": "blocked-stronger-evidence",
        "summary": (
            "The closure lane compares the mid-line write model with one "
            "audited UAE-family package. Copperline and vAmiga cannot answer "
            "these cases, so promotion requires another audited family or "
            "hardware capture."
        ),
        "documents": [
            "knowledge/processes/amiga-programmable-hblank-write-timing.md",
        ],
    },
    {
        "id": "paula-cross-producer-raw-waveform",
        "kind": "assertion-boundary",
        "classification": "scoped-out",
        "summary": (
            "Raw samples and absolute RMS are not compared across different "
            "filter, gain, phase, and resampling paths; the asserted boundary "
            "is routing, cadence, and within-producer volume ratio."
        ),
        "documents": [
            "knowledge/processes/amiga-paula-audio-conformance.md",
        ],
    },
]


@dataclasses.dataclass(frozen=True)
class EnvRequirement:
    name: str
    kind: str


@dataclasses.dataclass(frozen=True)
class Lane:
    id: str
    argv: tuple[str, ...]
    required_environment: tuple[EnvRequirement, ...] = ()
    environment: tuple[tuple[str, str], ...] = ()
    validator: str | None = None


FILE = "file"
DIRECTORY = "directory"


LANES = (
    Lane(
        id="amiga-regressions",
        argv=("scripts/verify-amiga-regressions.sh",),
    ),
    Lane(
        id="snapshot-roundtrip",
        argv=(
            "cargo",
            "test",
            "--locked",
            "-p",
            "runtime-commodore-amiga",
            "--test",
            "snapshot_roundtrip",
        ),
    ),
    Lane(
        id="test-kit-v1.12",
        argv=("scripts/verify-amiga-test-kit.sh",),
        required_environment=(
            EnvRequirement("EMU198X_AMIGA_TEST_KIT_ADF", FILE),
        ),
    ),
    Lane(
        id="test-kit-v1.21-ocs",
        argv=("scripts/verify-amiga-test-kit-video.sh",),
        required_environment=(
            EnvRequirement("EMU198X_AMIGA_TEST_KIT_V121_ADF", FILE),
        ),
        validator="test-kit-v1.21-ocs",
    ),
    Lane(
        id="test-kit-v1.21-aga",
        argv=("scripts/verify-amiga-test-kit-video-a1200.sh",),
        required_environment=(
            EnvRequirement("EMU198X_AMIGA_TEST_KIT_V121_ADF", FILE),
        ),
        validator="test-kit-v1.21-aga",
    ),
    Lane(
        id="paula-audio",
        argv=("scripts/verify-amiga-paula-audio.sh",),
    ),
    Lane(
        id="programmable-hblank",
        argv=("scripts/verify-amiga-programmable-hblank.sh",),
    ),
    Lane(
        id="programmable-hblank-write-timing",
        argv=("scripts/verify-amiga-programmable-hblank-write-timing.sh",),
    ),
    Lane(
        id="golden-matrix",
        argv=("scripts/verify-amiga-golden-matrix.sh",),
        required_environment=(
            EnvRequirement("EMU198X_AMIGA_A1000_KICKSTART_DISK", FILE),
        ),
    ),
    Lane(
        id="catalogue-ten",
        argv=("scripts/verify-amiga-catalogue.sh",),
        required_environment=(
            EnvRequirement("EMU198X_CATALOGUE_MEDIA_ROOT", DIRECTORY),
            EnvRequirement("EMU198X_CATALOGUE_FIRMWARE_ROOT", DIRECTORY),
        ),
        validator="catalogue-ten",
    ),
)

LANE_BY_ID = {lane.id: lane for lane in LANES}

PATH_ENV_NAMES = {
    "HOME",
    "TMPDIR",
    "CARGO_TARGET_DIR",
    "EMU198X_AMIGA_A1000_KICKSTART_DISK",
    "EMU198X_AMIGA_KICKSTART_13_ROM",
    "EMU198X_AMIGA_KICKSTART_204_ROM",
    "EMU198X_AMIGA_KICKSTART_31_A1200_ROM",
    "EMU198X_AMIGA_ROM_DIR",
    "EMU198X_AMIGA_TEST_KIT_ADF",
    "EMU198X_AMIGA_TEST_KIT_V121_ADF",
    "EMU198X_CATALOGUE_FIRMWARE_ROOT",
    "EMU198X_CATALOGUE_MEDIA_ROOT",
}


class RunInterrupted(Exception):
    """Raised by the SIGTERM/SIGHUP handlers so evidence can be finalised."""


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .isoformat(timespec="milliseconds")
        .replace("+00:00", "Z")
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fsync_path(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def acquire_run_lock(path: Path) -> int:
    """Acquire the kernel-backed lock for one revision's mutable report."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        os.close(descriptor)
        raise RuntimeError(
            "another Amiga closure invocation is active for this revision"
        ) from error
    except BaseException:
        os.close(descriptor)
        raise
    try:
        os.ftruncate(descriptor, 0)
        os.write(descriptor, f"pid={os.getpid()}\n".encode("ascii"))
        os.fsync(descriptor)
    except BaseException:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)
        raise
    return descriptor


def release_run_lock(descriptor: int) -> None:
    try:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
    finally:
        os.close(descriptor)


def atomic_write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            "w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as handle:
            temporary = Path(handle.name)
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        temporary = None
        try:
            fsync_path(path.parent)
        except OSError:
            return
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def git_output(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ("git", *args),
        cwd=repo_root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def git_state(repo_root: Path) -> tuple[str, bool]:
    revision = git_output(repo_root, "rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise RuntimeError("git did not return a full 40-character revision")
    porcelain = git_output(
        repo_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
    )
    return revision, bool(porcelain)


def selected_lanes(requested: Sequence[str]) -> list[Lane]:
    if not requested:
        return list(LANES)
    unknown = sorted(set(requested) - set(LANE_BY_ID))
    if unknown:
        raise ValueError(f"unknown lane ID(s): {', '.join(unknown)}")
    selected = set(requested)
    return [lane for lane in LANES if lane.id in selected]


def validate_registry(
    repo_root: Path,
    registry: Sequence[Mapping[str, object]] = DISAGREEMENT_REGISTRY,
) -> None:
    ids = [row.get("id") for row in registry]
    if tuple(ids) != EXPECTED_DISAGREEMENT_IDS:
        raise RuntimeError(
            "disagreement registry ID set or order differs from the closure contract"
        )
    if len(ids) != len(set(ids)):
        raise RuntimeError("disagreement registry IDs must be unique")
    invalid = {
        row.get("classification") for row in registry
    } - ALLOWED_DISAGREEMENT_CLASSIFICATIONS
    if invalid:
        raise RuntimeError(
            "invalid disagreement classification(s): "
            + ", ".join(sorted(str(value) for value in invalid))
        )

    root = repo_root.resolve()
    for row in registry:
        row_id = row["id"]
        for field in ("kind", "summary"):
            value = row.get(field)
            if not isinstance(value, str) or not value.strip():
                raise RuntimeError(f"disagreement {row_id} has no {field}")
        documents = row.get("documents")
        if not isinstance(documents, list) or not documents:
            raise RuntimeError(f"disagreement {row_id} has no document references")
        for document in documents:
            if not isinstance(document, str):
                raise RuntimeError(f"disagreement {row_id} has a non-string document")
            relative = Path(document)
            if relative.is_absolute() or ".." in relative.parts:
                raise RuntimeError(
                    f"disagreement {row_id} has an unsafe document path: {document}"
                )
            candidate = (repo_root / relative).resolve(strict=False)
            if not candidate.is_relative_to(root) or not candidate.is_file():
                raise RuntimeError(
                    f"disagreement {row_id} document is missing: {document}"
                )


def validate_required_environment(
    lanes: Iterable[Lane], environment: Mapping[str, str]
) -> list[str]:
    errors: list[str] = []
    seen: set[str] = set()
    for lane in lanes:
        for requirement in lane.required_environment:
            key = f"{requirement.name}:{requirement.kind}"
            if key in seen:
                continue
            seen.add(key)
            raw = environment.get(requirement.name)
            if not raw:
                errors.append(
                    f"{requirement.name} is required for the selected closure lane(s)"
                )
                continue
            path = Path(raw).expanduser()
            if requirement.kind == FILE and (
                not path.is_file() or not os.access(path, os.R_OK)
            ):
                errors.append(f"{requirement.name} must name a readable file")
            elif requirement.kind == DIRECTORY and (
                not path.is_dir() or not os.access(path, os.R_OK | os.X_OK)
            ):
                errors.append(f"{requirement.name} must name a readable directory")
    return errors


def validate_commands(repo_root: Path, lanes: Iterable[Lane]) -> list[str]:
    errors: list[str] = []
    for lane in lanes:
        executable = lane.argv[0]
        if executable.startswith("scripts/"):
            path = repo_root / executable
            if not path.is_file():
                errors.append(f"{lane.id}: required wrapper is missing: {executable}")
            elif not os.access(path, os.X_OK):
                errors.append(f"{lane.id}: required wrapper is not executable: {executable}")
    return errors


def make_redactor(environment: Mapping[str, str]) -> Callable[[str], str]:
    replacements: list[tuple[str, str]] = []
    for name in sorted(PATH_ENV_NAMES):
        value = environment.get(name, "")
        if not value or value == os.sep:
            continue
        variants = {value}
        try:
            variants.add(str(Path(value).expanduser().resolve(strict=False)))
        except OSError:
            pass
        for variant in variants:
            if variant and variant != os.sep:
                replacements.append((variant, f"<redacted:{name}>"))
    replacements.sort(key=lambda pair: len(pair[0]), reverse=True)

    def redact(text: str) -> str:
        for value, replacement in replacements:
            text = text.replace(value, replacement)
        return text

    return redact


PASS_RE = re.compile(r"^\[PASS\]\s+(\S+)(?:\s|$)")
SNAP_PASS_RE = re.compile(r"^\[SNAP-PASS\]\s+(\S+)\s*$")


def validate_catalogue_log(path: Path) -> dict[str, object]:
    pass_ids: list[str] = []
    snapshot_ids: list[str] = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            line = raw_line.rstrip("\r\n")
            if match := PASS_RE.match(line):
                pass_ids.append(match.group(1))
            if match := SNAP_PASS_RE.match(line):
                snapshot_ids.append(match.group(1))

    expected_ids = list(EXPECTED_CATALOGUE_IDS)
    pass_ids_exact = pass_ids == expected_ids
    snapshot_ids_exact = snapshot_ids == expected_ids
    valid = pass_ids_exact and snapshot_ids_exact
    return {
        "id": "exact-reviewed-catalogue-and-snapshot-markers",
        "status": "pass" if valid else "fail",
        "expected_ids": expected_ids,
        "actual_pass_ids": pass_ids,
        "actual_snapshot_pass_ids": snapshot_ids,
        "expected_pass_markers": len(expected_ids),
        "actual_pass_markers": len(pass_ids),
        "expected_snapshot_pass_markers": len(expected_ids),
        "actual_snapshot_pass_markers": len(snapshot_ids),
        "pass_ids_unique": len(set(pass_ids)) == len(pass_ids),
        "snapshot_ids_unique": len(set(snapshot_ids)) == len(snapshot_ids),
        "marker_id_sets_equal": set(pass_ids) == set(snapshot_ids),
        "pass_ids_exact_and_ordered": pass_ids_exact,
        "snapshot_ids_exact_and_ordered": snapshot_ids_exact,
    }


def validate_test_kit_v121_log(
    path: Path, validator_id: str
) -> dict[str, object]:
    expected = EXPECTED_TEST_KIT_V121_MARKERS.get(validator_id)
    if expected is None:
        raise ValueError(f"unknown Test Kit v1.21 validator: {validator_id}")

    marker_prefix = "Amiga Test Kit v1.21 "
    actual: list[str] = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            if (offset := raw_line.find(marker_prefix)) >= 0:
                actual.append(raw_line[offset:].rstrip("\r\n"))

    expected_list = list(expected)
    exact_and_ordered = actual == expected_list
    return {
        "id": f"{validator_id}-exact-contract-markers",
        "status": "pass" if exact_and_ordered else "fail",
        "expected_markers": expected_list,
        "actual_markers": actual,
        "expected_marker_count": len(expected_list),
        "actual_marker_count": len(actual),
        "markers_unique": len(set(actual)) == len(actual),
        "markers_exact_and_ordered": exact_and_ordered,
    }


def validate_lane_log(lane: Lane, path: Path) -> dict[str, object] | None:
    if lane.validator is None:
        return None
    if lane.validator == "catalogue-ten":
        return validate_catalogue_log(path)
    if lane.validator in EXPECTED_TEST_KIT_V121_MARKERS:
        return validate_test_kit_v121_log(path, lane.validator)
    raise RuntimeError(
        f"lane {lane.id} names unknown log validator {lane.validator}"
    )


def next_attempt_number(lane_record: dict[str, object]) -> int:
    attempts = lane_record.setdefault("attempts", [])
    if not isinstance(attempts, list):
        raise RuntimeError("report lane attempts must be a list")
    return len(attempts) + 1


def lane_records() -> list[dict[str, object]]:
    return [
        {
            "command_id": lane.id,
            "required_environment": [
                requirement.name for requirement in lane.required_environment
            ],
            "fixed_environment": {key: value for key, value in lane.environment},
            "status": "not-run",
            "attempts": [],
        }
        for lane in LANES
    ]


def new_report(revision: str) -> dict[str, object]:
    return {
        "schema": SCHEMA,
        "revision": revision,
        "evidence_scope": EVIDENCE_SCOPE,
        "started_at_utc": None,
        "ended_at_utc": None,
        "dirty": None,
        "status": "not-run",
        "lanes": lane_records(),
        "invocations": [],
        "disagreement_registry": DISAGREEMENT_REGISTRY,
    }


def load_or_create_report(path: Path, revision: str) -> dict[str, object]:
    if not path.exists():
        return new_report(revision)
    with path.open("r", encoding="utf-8") as handle:
        report = json.load(handle)
    if report.get("schema") != SCHEMA:
        raise RuntimeError(f"existing {REPORT_FILENAME} has an unsupported schema")
    if report.get("revision") != revision:
        raise RuntimeError(f"existing {REPORT_FILENAME} belongs to another revision")
    # The executable owns the registry. Refreshing it cannot rewrite attempts.
    report["disagreement_registry"] = DISAGREEMENT_REGISTRY
    return report


def find_lane_record(report: dict[str, object], lane_id: str) -> dict[str, object]:
    lanes = report.get("lanes")
    if not isinstance(lanes, list):
        raise RuntimeError("report lanes must be a list")
    for lane in lanes:
        if isinstance(lane, dict) and lane.get("command_id") == lane_id:
            return lane
    raise RuntimeError(f"report has no lane record for {lane_id}")


def recompute_report_status(report: dict[str, object]) -> str:
    lane_values = report.get("lanes")
    if not isinstance(lane_values, list):
        raise RuntimeError("report lanes must be a list")
    statuses = [lane.get("status") for lane in lane_values if isinstance(lane, dict)]
    if statuses and all(status == "pass" for status in statuses):
        return "pass"
    if any(status in {"fail", "error", "interrupted"} for status in statuses):
        return "fail"
    return "incomplete"


def referenced_report_logs(
    output_root: Path, report: Mapping[str, object]
) -> list[tuple[Path, Path]]:
    output_resolved = output_root.resolve()
    lane_values = report.get("lanes")
    if not isinstance(lane_values, list):
        raise RuntimeError("report lanes must be a list")
    command_ids = [
        lane.get("command_id") if isinstance(lane, dict) else None
        for lane in lane_values
    ]
    if command_ids != [lane.id for lane in LANES]:
        raise RuntimeError("report lane set or order differs from the closure contract")
    logs: dict[str, tuple[Path, Path]] = {}
    for lane in lane_values:
        if not isinstance(lane, dict):
            raise RuntimeError("report lane must be an object")
        attempts = lane.get("attempts")
        if not isinstance(attempts, list):
            raise RuntimeError("report lane attempts must be a list")
        if (
            lane.get("status") != "pass"
            or not attempts
            or not isinstance(attempts[-1], dict)
            or attempts[-1].get("status") != "pass"
        ):
            raise RuntimeError(
                f"lane {lane.get('command_id', '<unknown>')} has no latest passing attempt"
            )
        if attempts[-1].get("dirty") is not False:
            raise RuntimeError(
                f"lane {lane.get('command_id', '<unknown>')} latest attempt is dirty"
            )
        if attempts[-1].get("revision") != report.get("revision"):
            raise RuntimeError(
                f"lane {lane.get('command_id', '<unknown>')} latest attempt has another revision"
            )
        if attempts[-1].get("exit_code") != 0:
            raise RuntimeError(
                f"lane {lane.get('command_id', '<unknown>')} latest passing "
                "attempt has a non-zero or missing exit code"
            )
        command_id = lane.get("command_id")
        contract_lane = LANE_BY_ID.get(command_id) if isinstance(command_id, str) else None
        if contract_lane is None:
            raise RuntimeError(f"report names unknown lane {command_id}")
        stored_validation: dict[str, object] | None = None
        if contract_lane.validator is not None:
            validation = attempts[-1].get("validation")
            if not isinstance(validation, dict) or validation.get("status") != "pass":
                raise RuntimeError(
                    f"{command_id} latest attempt has no passing marker validation"
                )
            stored_validation = validation
        for attempt_index, attempt in enumerate(attempts):
            if not isinstance(attempt, dict):
                raise RuntimeError("report lane attempt must be an object")
            if attempt.get("command_id") != lane.get("command_id"):
                raise RuntimeError("report lane attempt command ID differs from its lane")
            raw_relative = attempt.get("log")
            if not isinstance(raw_relative, str):
                raise RuntimeError("report lane attempt has no relative log path")
            relative = Path(raw_relative)
            if relative.is_absolute() or ".." in relative.parts:
                raise RuntimeError("report lane attempt has an unsafe log path")
            source = output_root / relative
            try:
                source_resolved = source.resolve(strict=True)
            except FileNotFoundError as error:
                raise RuntimeError(f"referenced log is missing: {raw_relative}") from error
            if not source_resolved.is_relative_to(output_resolved) or not source.is_file():
                raise RuntimeError(f"referenced log is outside the report: {raw_relative}")
            expected_sha256 = attempt.get("log_sha256")
            if not isinstance(expected_sha256, str) or not re.fullmatch(
                r"[0-9a-f]{64}", expected_sha256
            ):
                raise RuntimeError(f"invalid or missing log SHA-256: {raw_relative}")
            if sha256_file(source) != expected_sha256:
                raise RuntimeError(f"referenced log SHA-256 differs: {raw_relative}")
            if (
                attempt_index == len(attempts) - 1
                and stored_validation is not None
            ):
                recomputed_validation = validate_lane_log(contract_lane, source)
                if recomputed_validation != stored_validation:
                    raise RuntimeError(
                        f"{command_id} stored marker validation differs from "
                        "the hashed latest log"
                    )
            logs[relative.as_posix()] = (source, relative)
    return [logs[key] for key in sorted(logs)]


def archive_passing_report(
    repo_root: Path,
    output_root: Path,
    report: Mapping[str, object],
) -> Path:
    if report.get("status") != "pass":
        raise RuntimeError("only an overall passing closure report may be archived")
    if report.get("dirty") is not False:
        raise RuntimeError("a dirty-worktree closure report cannot be archived")
    revision = report.get("revision")
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise RuntimeError("closure report has no full Git revision")
    if output_root.name != revision:
        raise RuntimeError("closure report revision does not match its output directory")
    report_path = output_root / REPORT_FILENAME
    if not report_path.is_file():
        raise RuntimeError("closure report JSON is missing")
    with report_path.open("r", encoding="utf-8") as handle:
        on_disk = json.load(handle)
    if on_disk != report:
        raise RuntimeError("in-memory closure report differs from report.json")

    logs = referenced_report_logs(output_root, report)
    archive_root = repo_root / "test-data" / "commodore" / "amiga" / "closure-reports"
    archive_root.mkdir(parents=True, exist_ok=True)
    destination = archive_root / revision
    lock_path = archive_root / f".{revision}.archive.lock"
    lock_descriptor: int | None = None
    staging: Path | None = None
    try:
        try:
            lock_descriptor = os.open(
                lock_path,
                os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                0o600,
            )
        except FileExistsError as error:
            raise RuntimeError(f"archive publication is already active for {revision}") from error
        if destination.exists():
            raise FileExistsError(f"closure archive already exists for {revision}")
        staging = Path(
            tempfile.mkdtemp(
                prefix=f".{revision}.", suffix=".tmp", dir=archive_root
            )
        )
        archived_report = staging / REPORT_FILENAME
        shutil.copy2(report_path, archived_report)
        fsync_path(archived_report)
        for source, relative in logs:
            archived_log = staging / relative
            archived_log.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, archived_log)
            fsync_path(archived_log)
        logs_directory = staging / "logs"
        if logs_directory.is_dir():
            fsync_path(logs_directory)
        fsync_path(staging)
        os.rename(staging, destination)
        staging = None
        fsync_path(archive_root)
        return destination
    finally:
        if lock_descriptor is not None:
            os.close(lock_descriptor)
            lock_path.unlink(missing_ok=True)
        if staging is not None:
            shutil.rmtree(staging, ignore_errors=True)


def terminate_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGINT)
        process.wait(timeout=5)
        return
    except (ProcessLookupError, subprocess.TimeoutExpired):
        pass
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
        return
    except (ProcessLookupError, subprocess.TimeoutExpired):
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        return
    process.wait()


def run_lane(
    repo_root: Path,
    output_root: Path,
    lane: Lane,
    revision: str,
    dirty: bool,
    environment: Mapping[str, str],
    lane_record: dict[str, object],
    report: dict[str, object],
    report_path: Path,
) -> dict[str, object]:
    attempt_number = next_attempt_number(lane_record)
    relative_log = Path("logs") / f"{lane.id}-attempt-{attempt_number:03d}.log"
    log_path = output_root / relative_log
    log_path.parent.mkdir(parents=True, exist_ok=True)

    started_at = utc_now()
    started_monotonic = time.monotonic()
    attempt: dict[str, object] = {
        "command_id": lane.id,
        "revision": revision,
        "dirty": dirty,
        "started_at_utc": started_at,
        "ended_at_utc": None,
        "exit_code": None,
        "duration_seconds": None,
        "log": relative_log.as_posix(),
        "log_sha256": None,
        "status": "running",
    }
    attempts = lane_record["attempts"]
    assert isinstance(attempts, list)
    attempts.append(attempt)
    lane_record["status"] = "running"
    report["status"] = "running"
    atomic_write_json(report_path, report)

    child_environment = dict(environment)
    child_environment.pop("EMU198X_UPDATE_GOLDENS", None)
    child_environment.update(lane.environment)
    child_environment["EMU198X_ACCURACY_GIT_REVISION"] = revision
    child_environment["EMU198X_ACCURACY_GIT_DIRTY"] = "1" if dirty else "0"
    redact = make_redactor(child_environment)

    process: subprocess.Popen[str] | None = None
    interrupted = False
    launch_error: str | None = None
    exit_code: int | None = None
    with log_path.open("w", encoding="utf-8", buffering=1) as log:
        log.write(f"[closure] command_id={lane.id}\n")
        log.write(f"[closure] revision={revision}\n")
        log.write(f"[closure] dirty={str(dirty).lower()}\n")
        log.write(f"[closure] started_at_utc={started_at}\n")
        try:
            process = subprocess.Popen(
                lane.argv,
                cwd=repo_root,
                env=child_environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                start_new_session=True,
            )
            assert process.stdout is not None
            for line in process.stdout:
                safe_line = redact(line)
                log.write(safe_line)
                sys.stdout.write(safe_line)
                sys.stdout.flush()
            exit_code = process.wait()
        except (KeyboardInterrupt, RunInterrupted):
            interrupted = True
            if process is not None:
                terminate_process(process)
                exit_code = process.returncode
            log.write("[closure] interrupted\n")
        except OSError as error:
            launch_error = redact(str(error))
            log.write(f"[closure] launch_error={launch_error}\n")

    ended_at = utc_now()
    attempt["ended_at_utc"] = ended_at
    attempt["exit_code"] = exit_code
    attempt["duration_seconds"] = round(time.monotonic() - started_monotonic, 3)
    attempt["log_sha256"] = sha256_file(log_path)

    validation = validate_lane_log(lane, log_path)
    if validation is not None:
        attempt["validation"] = validation

    if interrupted:
        status = "interrupted"
    elif launch_error is not None:
        status = "error"
        attempt["error"] = "command could not be started"
    elif exit_code != 0:
        status = "fail"
    elif validation is not None and validation["status"] != "pass":
        status = "fail"
    else:
        status = "pass"

    attempt["status"] = status
    lane_record["status"] = status
    report["status"] = recompute_report_status(report)
    atomic_write_json(report_path, report)

    if interrupted:
        raise RunInterrupted
    return attempt


def install_signal_handlers() -> dict[int, object]:
    previous: dict[int, object] = {}

    def interrupt(_signum: int, _frame: object) -> None:
        raise RunInterrupted

    for signum in (signal.SIGTERM, signal.SIGHUP):
        previous[signum] = signal.getsignal(signum)
        signal.signal(signum, interrupt)
    return previous


def restore_signal_handlers(previous: Mapping[int, object]) -> None:
    for signum, handler in previous.items():
        signal.signal(signum, handler)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run revision-stamped Amiga accuracy-closure evidence lanes."
    )
    parser.add_argument(
        "--lane",
        action="append",
        default=[],
        metavar="ID",
        help="run one named lane; repeat to retry several lanes",
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow a diagnostic run from a dirty tree (clean is required by default)",
    )
    parser.add_argument(
        "--list-lanes",
        action="store_true",
        help="list lane IDs without running them",
    )
    parser.add_argument(
        "--archive-passing-report",
        action="store_true",
        help=(
            "after an overall pass, atomically retain report and logs below "
            "test-data without overwriting an existing revision"
        ),
    )
    return parser.parse_args(argv)


def execute_closure(
    args: argparse.Namespace,
    repo_root: Path,
    selected: Sequence[Lane],
    revision: str,
    dirty: bool,
    output_root: Path,
) -> int:
    report_path = output_root / REPORT_FILENAME
    try:
        report = load_or_create_report(report_path, revision)
    except (OSError, json.JSONDecodeError, RuntimeError) as error:
        print(f"error: cannot load closure report: {error}", file=sys.stderr)
        return 2

    invocation_started = utc_now()
    invocations = report.setdefault("invocations", [])
    if not isinstance(invocations, list):
        print("error: report invocations must be a list", file=sys.stderr)
        return 2
    invocation: dict[str, object] = {
        "started_at_utc": invocation_started,
        "ended_at_utc": None,
        "dirty": dirty,
        "selected_lanes": [lane.id for lane in selected],
        "status": "running",
    }
    invocations.append(invocation)
    if report.get("started_at_utc") is None:
        report["started_at_utc"] = invocation_started
    report["ended_at_utc"] = None
    report["dirty"] = dirty
    report["status"] = "running"
    atomic_write_json(report_path, report)

    print(f"Amiga closure revision: {revision}")
    print(f"Dirty worktree: {str(dirty).lower()}")
    print(f"Report: target/accuracy/amiga-closure/{revision}/{REPORT_FILENAME}")

    interrupted = False
    selected_failed = False
    previous_handlers = install_signal_handlers()
    try:
        for lane in selected:
            print(f"\n== {lane.id} ==")
            lane_record = find_lane_record(report, lane.id)
            try:
                attempt = run_lane(
                    repo_root,
                    output_root,
                    lane,
                    revision,
                    dirty,
                    os.environ,
                    lane_record,
                    report,
                    report_path,
                )
            except (KeyboardInterrupt, RunInterrupted):
                interrupted = True
                selected_failed = True
                print(f"INTERRUPTED {lane.id}", file=sys.stderr)
                break
            status = attempt["status"]
            print(f"{str(status).upper()} {lane.id}")
            if status != "pass":
                selected_failed = True

            try:
                current_revision, current_dirty = git_state(repo_root)
            except (OSError, subprocess.CalledProcessError, RuntimeError):
                current_revision, current_dirty = "unknown", True
            if current_revision != revision or current_dirty != dirty:
                selected_failed = True
                lane_record["status"] = "fail"
                attempt["status"] = "fail"
                attempt["repository_state_changed"] = True
                report["status"] = recompute_report_status(report)
                atomic_write_json(report_path, report)
                print(
                    "FAIL repository state changed during the closure run",
                    file=sys.stderr,
                )
                break
    finally:
        restore_signal_handlers(previous_handlers)

    invocation["ended_at_utc"] = utc_now()
    invocation["status"] = (
        "interrupted" if interrupted else "fail" if selected_failed else "pass"
    )
    report["ended_at_utc"] = invocation["ended_at_utc"]
    report["status"] = recompute_report_status(report)
    atomic_write_json(report_path, report)

    if interrupted:
        return 130
    if args.archive_passing_report:
        try:
            final_revision, final_dirty = git_state(repo_root)
        except (OSError, subprocess.CalledProcessError, RuntimeError) as error:
            print(
                f"error: cannot recheck repository state before archive: {error}",
                file=sys.stderr,
            )
            return 1
        if final_revision != revision or final_dirty:
            print(
                "error: cannot archive because the repository is no longer clean at "
                f"revision {revision}",
                file=sys.stderr,
            )
            return 1
        try:
            archive = archive_passing_report(repo_root, output_root, report)
        except (OSError, json.JSONDecodeError, RuntimeError) as error:
            print(f"error: cannot archive closure report: {error}", file=sys.stderr)
            return 1
        relative_archive = archive.relative_to(repo_root)
        archive_bytes = sum(
            path.stat().st_size for path in archive.rglob("*") if path.is_file()
        )
        print(f"Archived: {relative_archive} ({archive_bytes} bytes)")
    return 1 if selected_failed else 0


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    repo_root = Path(__file__).resolve().parent.parent
    try:
        validate_registry(repo_root)
    except RuntimeError as error:
        print(f"error: invalid disagreement registry: {error}", file=sys.stderr)
        return 2

    if args.list_lanes:
        for lane in LANES:
            print(lane.id)
        return 0

    try:
        selected = selected_lanes(args.lane)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    try:
        revision, dirty = git_state(repo_root)
    except (OSError, subprocess.CalledProcessError, RuntimeError) as error:
        print(f"error: cannot identify repository state: {error}", file=sys.stderr)
        return 2

    if dirty and not args.allow_dirty:
        print(
            "error: the Amiga closure runner requires a clean Git worktree; "
            "commit or remove changes, or use --allow-dirty for diagnosis",
            file=sys.stderr,
        )
        return 2

    preflight_errors = validate_commands(repo_root, selected)
    preflight_errors.extend(validate_required_environment(selected, os.environ))
    if preflight_errors:
        for error in preflight_errors:
            print(f"error: {error}", file=sys.stderr)
        return 2

    output_root = repo_root / "target" / "accuracy" / "amiga-closure" / revision
    run_lock_path = output_root.parent / f".{revision}.run.lock"
    try:
        run_lock = acquire_run_lock(run_lock_path)
    except (OSError, RuntimeError) as error:
        print(f"error: cannot acquire closure run lock: {error}", file=sys.stderr)
        return 2
    try:
        return execute_closure(
            args,
            repo_root,
            selected,
            revision,
            dirty,
            output_root,
        )
    finally:
        release_run_lock(run_lock)


if __name__ == "__main__":
    raise SystemExit(main())

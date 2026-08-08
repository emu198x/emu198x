#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Validate cross-field semantics in a sprite-phase capture record."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


class CaptureValidationError(ValueError):
    """Raised when a capture contradicts the suite or its own measurements."""


def _object(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CaptureValidationError(f"{path} must be an object")
    return value


def _array(value: Any, path: str) -> list[Any]:
    if not isinstance(value, list):
        raise CaptureValidationError(f"{path} must be an array")
    return value


def _integer(value: Any, path: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise CaptureValidationError(f"{path} must be an integer")
    return value


def _field_numbers(value: Any, path: str) -> list[int]:
    fields = [
        _integer(field, f"{path}[{index}]")
        for index, field in enumerate(_array(value, path))
    ]
    if len(fields) != len(set(fields)):
        raise CaptureValidationError(f"{path} must not contain duplicate fields")
    return fields


def _consecutive(fields: list[int]) -> bool:
    return all(right == left + 1 for left, right in zip(fields, fields[1:]))


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _bound_path(root: Path, name_value: Any, path: str) -> Path:
    if not isinstance(name_value, str) or not name_value:
        raise CaptureValidationError(f"{path} must name a package-relative file")
    relative = Path(name_value)
    if relative.is_absolute() or ".." in relative.parts:
        raise CaptureValidationError(f"{path} must be package-relative without '..'")
    resolved_root = root.resolve()
    resolved = (resolved_root / relative).resolve()
    if resolved != resolved_root and resolved_root not in resolved.parents:
        raise CaptureValidationError(f"{path} resolves outside its package")
    return resolved


def _matching_suite_case(
    suite: dict[str, Any], case_id: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    cases = _array(suite.get("cases"), "suite.cases")
    matching_cases = [
        _object(case, f"suite.cases[{index}]")
        for index, case in enumerate(cases)
        if isinstance(case, dict) and case.get("id") == case_id
    ]
    if len(matching_cases) != 1:
        raise CaptureValidationError(
            f"suite must contain exactly one case named {case_id!r}"
        )

    artifacts = _array(suite.get("artifacts"), "suite.artifacts")
    matching_artifacts = [
        _object(artifact, f"suite.artifacts[{index}]")
        for index, artifact in enumerate(artifacts)
        if isinstance(artifact, dict) and artifact.get("case_id") == case_id
    ]
    if len(matching_artifacts) != 1:
        raise CaptureValidationError(
            f"suite must contain exactly one artifact for {case_id!r}"
        )
    return matching_cases[0], matching_artifacts[0]


def _validate_interval(
    interval_value: Any,
    path: str,
    capture_width: int,
) -> tuple[int, int] | None:
    interval = _object(interval_value, path)
    status = interval.get("status")
    start = interval.get("start_sample")
    stop = interval.get("stop_sample")
    if status == "observed":
        start_value = _integer(start, f"{path}.start_sample")
        stop_value = _integer(stop, f"{path}.stop_sample")
        if not 0 <= start_value < stop_value <= capture_width:
            raise CaptureValidationError(
                f"{path} must satisfy 0 <= start_sample < stop_sample <= "
                "source_capture.width"
            )
        return start_value, stop_value
    if status not in {"not-observed", "unmeasurable"}:
        raise CaptureValidationError(f"{path}.status is not recognised")
    if start is not None or stop is not None:
        raise CaptureValidationError(
            f"{path} endpoints must be null unless status is observed"
        )
    return None


def validate_capture_record(capture_value: Any, suite_value: Any) -> None:
    """Reject a schema-shaped record whose linked observations contradict."""

    capture = _object(capture_value, "capture")
    suite = _object(suite_value, "suite")
    suite_identity = _object(suite.get("suite"), "suite.suite")

    if capture.get("schema_version") != "1.0.0":
        raise CaptureValidationError("capture.schema_version must be 1.0.0")
    if capture.get("suite_id") != suite_identity.get("id"):
        raise CaptureValidationError("capture.suite_id does not match the suite")
    if capture.get("suite_version") != suite_identity.get("version"):
        raise CaptureValidationError("capture.suite_version does not match the suite")

    case_id = capture.get("case_id")
    if not isinstance(case_id, str):
        raise CaptureValidationError("capture.case_id must be a string")
    suite_case, suite_artifact = _matching_suite_case(suite, case_id)

    artifact = _object(capture.get("artifact"), "capture.artifact")
    artifact_sha = _object(suite_artifact.get("sha256"), "suite artifact sha256")
    artifact_bindings = (
        ("adf_file", "adf_file"),
        ("adf_sha256", "adf"),
        ("payload_file", "payload_file"),
        ("payload_sha256", "payload"),
    )
    for capture_key, suite_key in artifact_bindings:
        expected = (
            artifact_sha.get(suite_key)
            if suite_key in {"adf", "payload"}
            else suite_artifact.get(suite_key)
        )
        if artifact.get(capture_key) != expected:
            raise CaptureValidationError(
                f"capture.artifact.{capture_key} does not match the suite artifact"
            )

    applicability = _object(suite_case.get("applicability"), "suite case applicability")
    machine = _object(capture.get("machine"), "capture.machine")
    if machine.get("chipset") not in _array(
        applicability.get("chipsets"), "suite case applicability.chipsets"
    ):
        raise CaptureValidationError("capture.machine.chipset is outside suite applicability")
    if machine.get("region") not in _array(
        applicability.get("regions"), "suite case applicability.regions"
    ):
        raise CaptureValidationError("capture.machine.region is outside suite applicability")
    chip_ram = _integer(machine.get("chip_ram_bytes"), "capture.machine.chip_ram_bytes")
    minimum_chip_ram = _integer(
        applicability.get("min_chip_ram_bytes"),
        "suite case applicability.min_chip_ram_bytes",
    )
    if chip_ram < minimum_chip_ram:
        raise CaptureValidationError("capture.machine.chip_ram_bytes is below the suite minimum")

    execution = _object(capture.get("execution"), "capture.execution")
    ready_rule = _object(execution.get("ready_rule"), "capture.execution.ready_rule")
    suite_capture = _object(suite_case.get("capture"), "suite case capture")
    expected_ready_rule = {
        "record_address": suite_capture.get("ready_record_address"),
        "magic": suite_capture.get("ready_magic"),
        "case_number": suite_case.get("numeric_id"),
        "schema_version": 1,
        "byte_order": suite_capture.get("byte_order"),
    }
    for key, expected in expected_ready_rule.items():
        if ready_rule.get(key) != expected:
            raise CaptureValidationError(
                f"capture.execution.ready_rule.{key} does not match the suite"
            )

    suite_settle = _integer(suite_capture.get("settle_fields"), "suite case settle_fields")
    ready_minimum = _integer(
        ready_rule.get("field_counter_minimum"),
        "capture.execution.ready_rule.field_counter_minimum",
    )
    ready_observed = _integer(
        execution.get("ready_observed_field"),
        "capture.execution.ready_observed_field",
    )
    settle_fields = _integer(execution.get("settle_fields"), "capture.execution.settle_fields")
    if ready_minimum < suite_settle or settle_fields < suite_settle:
        raise CaptureValidationError("capture settle and ready minima must meet the suite minimum")
    if ready_observed < ready_minimum:
        raise CaptureValidationError(
            "capture.execution.ready_observed_field precedes the declared ready minimum"
        )

    captured_fields = _field_numbers(
        execution.get("captured_fields"), "capture.execution.captured_fields"
    )
    minimum_capture_fields = _integer(
        suite_capture.get("capture_fields"), "suite case capture_fields"
    )
    if len(captured_fields) < minimum_capture_fields:
        raise CaptureValidationError("capture contains fewer fields than the suite requires")
    if captured_fields != sorted(captured_fields) or not _consecutive(captured_fields):
        raise CaptureValidationError(
            "capture.execution.captured_fields must be ordered adjacent fields"
        )
    if captured_fields[0] < max(ready_observed, settle_fields):
        raise CaptureValidationError("capture begins before its ready and settle bounds")
    if suite_capture.get("adjacent_field_stability_required") is True and execution.get(
        "adjacent_field_stability"
    ) != "confirmed":
        raise CaptureValidationError("the suite requires confirmed adjacent-field stability")

    source_capture = _object(capture.get("source_capture"), "capture.source_capture")
    width = _integer(source_capture.get("width"), "capture.source_capture.width")
    height = _integer(source_capture.get("height"), "capture.source_capture.height")
    if width <= 0 or height <= 0:
        raise CaptureValidationError("source capture dimensions must be positive")
    if source_capture.get("blanking_retained") is not True:
        raise CaptureValidationError("source capture must retain blanking")
    if source_capture.get("overscan_retained") is not True:
        raise CaptureValidationError("source capture must retain overscan")

    normalization = _object(capture.get("normalization"), "capture.normalization")
    if normalization.get("alignment_search") is not False:
        raise CaptureValidationError("capture normalization may not search alignment")
    measurement_crop = normalization.get("measurement_crop")
    if measurement_crop is not None:
        crop = _object(measurement_crop, "capture.normalization.measurement_crop")
        crop_x = _integer(crop.get("x"), "measurement_crop.x")
        crop_y = _integer(crop.get("y"), "measurement_crop.y")
        crop_width = _integer(crop.get("width"), "measurement_crop.width")
        crop_height = _integer(crop.get("height"), "measurement_crop.height")
        if (
            crop_x < 0
            or crop_y < 0
            or crop_width <= 0
            or crop_height <= 0
            or crop_x + crop_width > width
            or crop_y + crop_height > height
        ):
            raise CaptureValidationError("measurement crop lies outside the source capture")

    beam_mapping = _object(normalization.get("beam_mapping"), "normalization.beam_mapping")
    sample_rows = _array(beam_mapping.get("sample_rows"), "beam_mapping.sample_rows")
    row_by_field: dict[int, int] = {}
    for index, row_value in enumerate(sample_rows):
        row = _object(row_value, f"beam_mapping.sample_rows[{index}]")
        field = _integer(row.get("field"), f"beam_mapping.sample_rows[{index}].field")
        sample_row = _integer(
            row.get("capture_row"), f"beam_mapping.sample_rows[{index}].capture_row"
        )
        if field in row_by_field:
            raise CaptureValidationError("beam mapping contains a duplicate field")
        if not 0 <= sample_row < height:
            raise CaptureValidationError("beam mapping sample row lies outside the capture")
        row_by_field[field] = sample_row
    if list(row_by_field) != captured_fields:
        raise CaptureValidationError(
            "beam mapping fields must exactly match execution.captured_fields in order"
        )

    observations = _object(capture.get("observations"), "capture.observations")
    observed_fields = _array(observations.get("fields"), "capture.observations.fields")
    if len(observed_fields) != len(captured_fields):
        raise CaptureValidationError(
            "observation count must equal execution.captured_fields count"
        )
    for index, observation_value in enumerate(observed_fields):
        path = f"capture.observations.fields[{index}]"
        observation = _object(observation_value, path)
        field = _integer(observation.get("field"), f"{path}.field")
        if field != captured_fields[index]:
            raise CaptureValidationError(
                "observation fields must match execution.captured_fields in order"
            )
        sample_row = _integer(observation.get("sample_row"), f"{path}.sample_row")
        if sample_row != row_by_field[field]:
            raise CaptureValidationError(
                f"{path}.sample_row does not match normalization.beam_mapping"
            )

        hblank_status = observation.get("hblank_status")
        hblank_stop = observation.get("hblank_stop_sample")
        if hblank_status == "observed":
            hblank_stop_value: int | None = _integer(hblank_stop, f"{path}.hblank_stop_sample")
            if not 0 <= hblank_stop_value < width:
                raise CaptureValidationError(f"{path}.hblank_stop_sample is outside the capture")
        elif hblank_status in {"not-observed", "unmeasurable"}:
            hblank_stop_value = None
            if hblank_stop is not None:
                raise CaptureValidationError(
                    f"{path}.hblank_stop_sample must be null unless observed"
                )
        else:
            raise CaptureValidationError(f"{path}.hblank_status is not recognised")

        marker = _validate_interval(observation.get("marker"), f"{path}.marker", width)
        sprite = _validate_interval(observation.get("sprite"), f"{path}.sprite", width)
        expected_hblank_delta = (
            sprite[0] - hblank_stop_value
            if sprite is not None and hblank_stop_value is not None
            else None
        )
        expected_marker_delta = (
            sprite[0] - marker[0] if sprite is not None and marker is not None else None
        )
        if observation.get("sprite_start_minus_hblank_stop_samples") != expected_hblank_delta:
            raise CaptureValidationError(
                f"{path}.sprite_start_minus_hblank_stop_samples does not equal the measured endpoints"
            )
        if observation.get("sprite_start_minus_marker_start_samples") != expected_marker_delta:
            raise CaptureValidationError(
                f"{path}.sprite_start_minus_marker_start_samples does not equal the measured endpoints"
            )

    producer = _object(capture.get("producer"), "capture.producer")
    provenance = _object(capture.get("provenance"), "capture.provenance")
    evidence_class = {
        "physical-hardware": "hardware-evidence",
        "software-emulator": "software-derived",
        "fpga-reimplementation": "fpga-derived",
    }.get(producer.get("kind"))
    if evidence_class is None:
        raise CaptureValidationError("capture.producer.kind is not recognised")
    classification = provenance.get("classification")
    if classification not in {evidence_class, "diagnostic-only"}:
        raise CaptureValidationError(
            "capture.provenance.classification contradicts capture.producer.kind"
        )


def verify_bound_files(
    capture_path: Path,
    suite_path: Path,
    capture: dict[str, Any],
) -> None:
    """Verify the raw capture and generated suite artifacts named by a record."""

    capture_root = capture_path.parent
    source_capture = _object(capture.get("source_capture"), "capture.source_capture")
    execution = _object(capture.get("execution"), "capture.execution")
    capture_files = (
        (
            source_capture.get("file_name"),
            source_capture.get("file_sha256"),
            "capture.source_capture.file_name",
            "raw capture",
        ),
        (
            source_capture.get("decoded_pixel_file_name"),
            source_capture.get("decoded_pixel_sha256"),
            "capture.source_capture.decoded_pixel_file_name",
            "decoded pixels",
        ),
        (
            execution.get("configuration_file_name"),
            execution.get("configuration_sha256"),
            "capture.execution.configuration_file_name",
            "capture configuration",
        ),
    )
    for name, expected_sha256, field_path, label in capture_files:
        bound = _bound_path(capture_root, name, field_path)
        if not bound.is_file():
            raise CaptureValidationError(f"{label} file is missing: {bound}")
        if _sha256(bound) != expected_sha256:
            raise CaptureValidationError(f"{label} SHA-256 differs: {bound}")

    artifact = _object(capture.get("artifact"), "capture.artifact")
    for file_key, hash_key in (
        ("adf_file", "adf_sha256"),
        ("payload_file", "payload_sha256"),
    ):
        artifact_path = _bound_path(
            suite_path.parent,
            artifact.get(file_key),
            f"capture.artifact.{file_key}",
        )
        if not artifact_path.is_file():
            raise CaptureValidationError(f"suite artifact is missing: {artifact_path}")
        if _sha256(artifact_path) != artifact.get(hash_key):
            raise CaptureValidationError(f"suite artifact SHA-256 differs: {artifact_path}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Validate semantic relationships and bound files in a sprite "
            "horizontal-phase capture record."
        )
    )
    parser.add_argument("capture", type=Path)
    parser.add_argument("--suite", type=Path, required=True)
    parser.add_argument(
        "--skip-file-checks",
        action="store_true",
        help="validate record semantics without opening named capture or artifact files",
    )
    args = parser.parse_args()

    capture_path = args.capture.resolve()
    suite_path = args.suite.resolve()
    capture = json.loads(capture_path.read_text(encoding="utf-8"))
    suite = json.loads(suite_path.read_text(encoding="utf-8"))
    validate_capture_record(capture, suite)
    if not args.skip_file_checks:
        verify_bound_files(capture_path, suite_path, capture)
    print(f"capture valid: {capture_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (CaptureValidationError, json.JSONDecodeError, OSError) as error:
        raise SystemExit(f"error: {error}") from error

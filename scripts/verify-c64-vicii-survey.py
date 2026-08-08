#!/usr/bin/env python3
"""Run the revision-keyed PAL 6569 VIC-II breadth survey."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
from typing import Callable, Mapping, Sequence


REPORT_SCHEMA = "org.198x.emu198x.c64-vicii-survey-report.v1"
PRODUCER_SCHEMA = "org.198x.emu198x.c64-vicii-survey-producer.v1"
ASSET_SCHEMA = "org.198x.emu198x.c64-vicii-survey-assets.v1"
ASSET_MANIFEST_ID = "vice-vicii-pal-6569-breadth-survey-assets-v1"
COMMAND_ID = "c64-vicii-pal-6569-breadth-survey"
REPORT_FILENAME = "report.json"
PRODUCER_FILENAME = "producer.json"
LOG_FILENAME = "survey.log"
TESTBENCH_ENV = "EMU198X_C64_VICII_TESTBENCH_DIR"
ROM_ENV = "EMU198X_C64_ROM_DIR"
EVIDENCE_SCOPE = (
    "This report records digital colour-index agreement for 13 selected PAL "
    "6569 VIC-II testbench programs at one Emu198x revision. It is not a "
    "general C64-accuracy, analogue-video or physical-hardware-conformance claim."
)

EXPECTED_CASES = (
    ("gfxfetch", "gfxfetch/gfxfetch.prg", "gfxfetch/references/gfxfetch.prg.png"),
    ("dmadelay", "dmadelay/test1-2a-03.prg", "dmadelay/references/test1-2a-03.prg.png"),
    ("colorfetchbug", "colorfetchbug/bitmap.prg", "colorfetchbug/references/bitmap.prg.png"),
    ("sequencer-bug", "sequencer-bug/bug.prg", "sequencer-bug/references/bug.prg.png"),
    ("greydot", "greydot/greydot.prg", "greydot/references/greydot.prg.png"),
    (
        "spritecrunch",
        "spritecrunch/spritecrunch-3b-00.prg",
        "spritecrunch/references/spritecrunch-3b-00.prg.png",
    ),
    ("spritedma", "spritedma/d017-54.prg", "spritedma/references/d017-54.prg.png"),
    (
        "spritefetchbug",
        "spritefetchbug/test-136-2a.prg",
        "spritefetchbug/references/test-136-2a.prg.png",
    ),
    (
        "sb_sprite_fetch",
        "sb_sprite_fetch/sbsprf24-163.prg",
        "sb_sprite_fetch/references/sbsprf24-163.prg.png",
    ),
    (
        "vicii_timing",
        "vicii_timing/vicii_reg_timing-a5.prg",
        "vicii_timing/references/vicii_reg_timing-a5.prg.png",
    ),
    ("videomode", "videomode/rmwtest.prg", "videomode/references/rmwtest.prg.png"),
    ("border", "border/border-250.prg", "border/references/border-250.prg.png"),
    ("screenpos", "screenpos/screenpos.prg", "screenpos/references/screenpos.prg.png"),
)

EXPECTED_FIRMWARE = (
    ("firmware:kernal", "kernal.rom"),
    ("firmware:basic", "basic.rom"),
    ("firmware:chargen", "chargen.rom"),
)

SURVEY_COMMAND = (
    "cargo",
    "test",
    "--locked",
    "--release",
    "-p",
    "runtime-commodore-c64",
    "--test",
    "vicii_testbench",
    "survey_testbench_categories",
    "--",
    "--ignored",
    "--exact",
    "--nocapture",
    "--test-threads=1",
)

SHA256_RE = re.compile(r"[0-9a-f]{64}")
REVISION_RE = re.compile(r"[0-9a-f]{40}")
PRODUCER_KEYS = {
    "schema",
    "revision",
    "dirty",
    "runtime_contract",
    "comparison_contract",
    "cases",
}
CASE_KEYS = {
    "id",
    "program",
    "reference",
    "reference_width",
    "reference_height",
    "reference_color_type",
    "reference_indexed_sha256",
    "actual_indexed_sha256",
    "matched_pixels",
    "total_pixels",
}


class VerificationError(RuntimeError):
    """Raised when an input or producer result violates the survey contract."""


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .isoformat(timespec="milliseconds")
        .replace("+00:00", "Z")
    )


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise VerificationError(f"JSON object repeats key {key!r}")
        result[key] = value
    return result


def decode_json_bytes(raw: bytes, label: str) -> dict[str, object]:
    try:
        value = json.loads(raw, object_pairs_hook=reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise VerificationError(f"{label} is not valid UTF-8 JSON: {error}") from error
    if not isinstance(value, dict):
        raise VerificationError(f"{label} must contain a JSON object")
    return value


def assert_safe_relative_path(raw: object, label: str) -> str:
    if not isinstance(raw, str) or not raw:
        raise VerificationError(f"{label} must be a non-empty relative path")
    path = Path(raw)
    if path.is_absolute() or ".." in path.parts or path.as_posix() != raw:
        raise VerificationError(f"{label} is not a safe canonical relative path")
    return raw


def expected_asset_contract() -> list[tuple[str, str, str, str]]:
    expected: list[tuple[str, str, str, str]] = []
    for case_id, program, reference in EXPECTED_CASES:
        expected.append((f"program:{case_id}", "program", "testbench", program))
        expected.append((f"reference:{case_id}", "reference", "testbench", reference))
    expected.extend(
        (asset_id, "firmware", "roms", relative)
        for asset_id, relative in EXPECTED_FIRMWARE
    )
    return expected


def load_and_verify_assets(
    manifest_path: Path,
    testbench_root: Path,
    rom_root: Path,
) -> dict[str, object]:
    try:
        raw = manifest_path.read_bytes()
    except OSError as error:
        raise VerificationError("cannot read the tracked VIC-II asset manifest") from error
    manifest = decode_json_bytes(raw, "asset manifest")
    if set(manifest) != {"schema", "id", "source", "scope", "assets"}:
        raise VerificationError("asset manifest top-level fields differ from v1")
    if manifest.get("schema") != ASSET_SCHEMA:
        raise VerificationError("asset manifest schema differs from v1")
    if manifest.get("id") != ASSET_MANIFEST_ID:
        raise VerificationError("asset manifest ID differs from v1")

    source = manifest.get("source")
    if (
        not isinstance(source, dict)
        or set(source) != {"suite", "holding", "upstream_revision"}
        or source.get("suite") != "VICE VIC-II testbench"
        or source.get("upstream_revision") != "unresolved"
    ):
        raise VerificationError("asset manifest must retain the unresolved upstream revision")
    scope = manifest.get("scope")
    expected_case_ids = [case_id for case_id, _, _ in EXPECTED_CASES]
    if (
        not isinstance(scope, dict)
        or set(scope) != {"model", "case_ids", "asset_count"}
        or scope.get("model") != "6569"
        or scope.get("case_ids") != expected_case_ids
        or scope.get("asset_count") != len(expected_asset_contract())
    ):
        raise VerificationError("asset manifest scope differs from the survey contract")

    assets = manifest.get("assets")
    if not isinstance(assets, list):
        raise VerificationError("asset manifest assets must be a list")
    expected = expected_asset_contract()
    if len(assets) != len(expected):
        raise VerificationError(f"asset manifest must contain exactly {len(expected)} assets")

    roots = {
        "testbench": testbench_root.resolve(strict=True),
        "roms": rom_root.resolve(strict=True),
    }
    verified: list[dict[str, object]] = []
    seen_ids: set[str] = set()
    for index, (asset, contract) in enumerate(zip(assets, expected, strict=True)):
        if not isinstance(asset, dict):
            raise VerificationError(f"asset {index} must be an object")
        if set(asset) != {"id", "role", "root", "relative_path", "bytes", "sha256"}:
            raise VerificationError(f"asset {index} fields differ from v1")
        expected_id, expected_role, expected_root, expected_relative = contract
        actual_contract = (
            asset.get("id"),
            asset.get("role"),
            asset.get("root"),
            asset.get("relative_path"),
        )
        if actual_contract != contract:
            raise VerificationError(
                f"asset {index} is not the expected {expected_id} contract"
            )
        if expected_id in seen_ids:
            raise VerificationError(f"asset manifest repeats {expected_id}")
        seen_ids.add(expected_id)
        relative = assert_safe_relative_path(expected_relative, f"asset {expected_id}")
        expected_bytes = asset.get("bytes")
        expected_sha256 = asset.get("sha256")
        if not isinstance(expected_bytes, int) or isinstance(expected_bytes, bool) or expected_bytes <= 0:
            raise VerificationError(f"asset {expected_id} has an invalid byte count")
        if not isinstance(expected_sha256, str) or SHA256_RE.fullmatch(expected_sha256) is None:
            raise VerificationError(f"asset {expected_id} has an invalid SHA-256")

        root = roots[expected_root]
        candidate = root / relative
        try:
            resolved = candidate.resolve(strict=True)
        except OSError as error:
            raise VerificationError(f"asset {expected_id} is missing") from error
        if not resolved.is_relative_to(root) or not resolved.is_file():
            raise VerificationError(f"asset {expected_id} is outside its configured root")
        try:
            actual_bytes = resolved.stat().st_size
            actual_sha256 = sha256_file(resolved)
        except OSError as error:
            raise VerificationError(f"cannot read asset {expected_id}") from error
        if actual_bytes != expected_bytes or actual_sha256 != expected_sha256:
            raise VerificationError(f"asset {expected_id} does not match its registered identity")
        verified.append(
            {
                "id": expected_id,
                "role": expected_role,
                "root": expected_root,
                "relative_path": expected_relative,
                "bytes": expected_bytes,
                "sha256": expected_sha256,
            }
        )

    return {
        "schema": ASSET_SCHEMA,
        "id": ASSET_MANIFEST_ID,
        "sha256": sha256_bytes(raw),
        "source": source,
        "scope": scope,
        "verified_asset_count": len(verified),
        "assets": verified,
    }


def require_contract_values(
    value: object,
    expected: Mapping[str, object],
    label: str,
) -> dict[str, object]:
    if not isinstance(value, dict):
        raise VerificationError(f"producer {label} must be an object")
    for key, expected_value in expected.items():
        if value.get(key) != expected_value:
            raise VerificationError(
                f"producer {label}.{key} differs from the v1 contract"
            )
    return value


def validate_producer(
    producer: dict[str, object],
    revision: str,
    dirty: bool,
) -> dict[str, object]:
    if set(producer) != PRODUCER_KEYS:
        raise VerificationError("producer top-level fields differ from v1")
    if producer.get("schema") != PRODUCER_SCHEMA:
        raise VerificationError("producer schema differs from v1")
    if producer.get("revision") != revision or producer.get("dirty") is not dirty:
        raise VerificationError("producer repository identity differs from the wrapper")

    runtime = require_contract_values(
        producer.get("runtime_contract"),
        {
            "model": "c64-pal-breadbin",
            "vic_model": "6569",
            "boot_frames": 150,
            "settle_frames": 60,
            "framebuffer_width": 416,
            "framebuffer_height": 312,
        },
        "runtime_contract",
    )
    comparison = require_contract_values(
        producer.get("comparison_contract"),
        {
            "method": "nearest-c64-palette-index-squared-rgb-v1",
            "crop_x": 16,
            "crop_y": 16,
            "reference_width": 384,
            "reference_height": 272,
        },
        "comparison_contract",
    )

    cases = producer.get("cases")
    if not isinstance(cases, list) or len(cases) != len(EXPECTED_CASES):
        raise VerificationError(f"producer must contain exactly {len(EXPECTED_CASES)} cases")
    normalised: list[dict[str, object]] = []
    for index, (case, contract) in enumerate(zip(cases, EXPECTED_CASES, strict=True)):
        if not isinstance(case, dict) or set(case) != CASE_KEYS:
            raise VerificationError(f"producer case {index} fields differ from v1")
        case_id, program, reference = contract
        if (
            case.get("id") != case_id
            or case.get("program") != program
            or case.get("reference") != reference
        ):
            raise VerificationError(f"producer case {index} differs from {case_id}")
        assert_safe_relative_path(case.get("program"), f"producer {case_id} program")
        assert_safe_relative_path(case.get("reference"), f"producer {case_id} reference")
        if (
            case.get("reference_width") != 384
            or case.get("reference_height") != 272
            or case.get("reference_color_type") != "rgba8"
        ):
            raise VerificationError(f"producer {case_id} reference format differs from v1")
        for key in ("reference_indexed_sha256", "actual_indexed_sha256"):
            value = case.get(key)
            if not isinstance(value, str) or SHA256_RE.fullmatch(value) is None:
                raise VerificationError(f"producer {case_id} has an invalid {key}")
        matched = case.get("matched_pixels")
        total = case.get("total_pixels")
        if (
            not isinstance(matched, int)
            or isinstance(matched, bool)
            or total != 384 * 272
            or not 0 <= matched <= total
        ):
            raise VerificationError(f"producer {case_id} has invalid pixel counts")
        measured = dict(case)
        measured["status"] = "measured"
        measured["match_percent"] = round(matched * 100.0 / total, 3)
        normalised.append(measured)

    return {
        "runtime_contract": runtime,
        "comparison_contract": comparison,
        "cases": normalised,
    }


def fsync_path(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
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
            pass
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
    if REVISION_RE.fullmatch(revision) is None:
        raise VerificationError("git did not return a full lowercase revision")
    porcelain = git_output(
        repo_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
    )
    return revision, bool(porcelain)


def require_dirty_policy(dirty: bool, allow_dirty: bool) -> None:
    if dirty and not allow_dirty:
        raise VerificationError(
            "the VIC-II survey requires a clean worktree; commit or remove changes, "
            "or use --allow-dirty for diagnosis"
        )


def required_directory(environment: Mapping[str, str], name: str) -> Path:
    raw = environment.get(name)
    if not raw:
        raise VerificationError(f"{name} must name a readable directory")
    path = Path(raw).expanduser()
    if not path.is_dir() or not os.access(path, os.R_OK | os.X_OK):
        raise VerificationError(f"{name} must name a readable directory")
    return path.resolve(strict=True)


def report_directory_name(revision: str, dirty: bool) -> str:
    return f"{revision}-dirty" if dirty else revision


def acquire_run_lock(path: Path) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        os.close(descriptor)
        raise VerificationError("another VIC-II survey is active for this revision") from error
    return descriptor


def release_run_lock(descriptor: int) -> None:
    try:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
    finally:
        os.close(descriptor)


def make_redactor(paths: Mapping[str, Path]) -> Callable[[str], str]:
    replacements = sorted(
        ((str(path), f"<redacted:{name}>") for name, path in paths.items()),
        key=lambda pair: len(pair[0]),
        reverse=True,
    )

    def redact(text: str) -> str:
        for value, replacement in replacements:
            text = text.replace(value, replacement)
        return text

    return redact


def run_command(
    repo_root: Path,
    output_root: Path,
    revision: str,
    dirty: bool,
    testbench_root: Path,
    rom_root: Path,
) -> tuple[int, float, str | None]:
    producer_path = output_root / PRODUCER_FILENAME
    producer_path.unlink(missing_ok=True)
    log_path = output_root / LOG_FILENAME
    environment = dict(os.environ)
    environment[TESTBENCH_ENV] = str(testbench_root)
    environment[ROM_ENV] = str(rom_root)
    environment["EMU198X_C64_VICII_SURVEY_RESULT"] = str(producer_path)
    environment["EMU198X_ACCURACY_GIT_REVISION"] = revision
    environment["EMU198X_ACCURACY_GIT_DIRTY"] = "true" if dirty else "false"
    environment["CARGO_TERM_COLOR"] = "never"
    redact = make_redactor({TESTBENCH_ENV: testbench_root, ROM_ENV: rom_root})

    started = time.monotonic()
    launch_error: str | None = None
    exit_code = 1
    with log_path.open("w", encoding="utf-8", buffering=1) as log:
        log.write(f"[survey] command_id={COMMAND_ID}\n")
        log.write(f"[survey] revision={revision}\n")
        log.write(f"[survey] dirty={str(dirty).lower()}\n")
        try:
            process = subprocess.Popen(
                SURVEY_COMMAND,
                cwd=repo_root,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
            )
            assert process.stdout is not None
            for line in process.stdout:
                safe = redact(line)
                log.write(safe)
                sys.stdout.write(safe)
                sys.stdout.flush()
            exit_code = process.wait()
        except OSError as error:
            launch_error = redact(str(error))
            log.write(f"[survey] launch_error={launch_error}\n")
    return exit_code, round(time.monotonic() - started, 3), launch_error


def base_report(
    revision: str,
    dirty: bool,
    started_at: str,
    fixture_manifest: dict[str, object],
) -> dict[str, object]:
    return {
        "schema": REPORT_SCHEMA,
        "revision": revision,
        "dirty": dirty,
        "status": "running",
        "evidence_scope": EVIDENCE_SCOPE,
        "command_id": COMMAND_ID,
        "started_at_utc": started_at,
        "ended_at_utc": None,
        "duration_seconds": None,
        "fixture_manifest": fixture_manifest,
        "execution": {
            "exit_code": None,
            "log": LOG_FILENAME,
            "log_sha256": None,
        },
        "producer": None,
        "runtime_contract": None,
        "comparison_contract": None,
        "cases": [],
        "error": None,
    }


def execute_survey(
    repo_root: Path,
    output_root: Path,
    revision: str,
    dirty: bool,
    testbench_root: Path,
    rom_root: Path,
    fixture_manifest: dict[str, object],
) -> int:
    output_root.mkdir(parents=True, exist_ok=True)
    report_path = output_root / REPORT_FILENAME
    started_at = utc_now()
    report = base_report(revision, dirty, started_at, fixture_manifest)
    atomic_write_json(report_path, report)

    exit_code, duration, launch_error = run_command(
        repo_root,
        output_root,
        revision,
        dirty,
        testbench_root,
        rom_root,
    )
    report["ended_at_utc"] = utc_now()
    report["duration_seconds"] = duration
    execution = report["execution"]
    assert isinstance(execution, dict)
    execution["exit_code"] = exit_code
    execution["log_sha256"] = sha256_file(output_root / LOG_FILENAME)

    error: str | None = launch_error
    producer_path = output_root / PRODUCER_FILENAME
    if error is None and exit_code != 0:
        error = "survey command failed"
    if error is None and not producer_path.is_file():
        error = "survey command produced no structured result"

    if error is None:
        try:
            producer_raw = producer_path.read_bytes()
            producer = decode_json_bytes(producer_raw, "survey producer result")
            measured = validate_producer(producer, revision, dirty)
            # Re-hash every input after execution so a mutable external holding
            # cannot change unnoticed while the measurement is in progress.
            after = load_and_verify_assets(
                repo_root / "test-data/commodore/c64/vicii-vice-survey/assets-v1.json",
                testbench_root,
                rom_root,
            )
            if after != fixture_manifest:
                raise VerificationError("verified fixture identity changed during the survey")
            current_revision, current_dirty = git_state(repo_root)
            if current_revision != revision or current_dirty != dirty:
                raise VerificationError("repository state changed during the survey")
            report["producer"] = {
                "schema": PRODUCER_SCHEMA,
                "file": PRODUCER_FILENAME,
                "sha256": sha256_bytes(producer_raw),
            }
            report["runtime_contract"] = measured["runtime_contract"]
            report["comparison_contract"] = measured["comparison_contract"]
            report["cases"] = measured["cases"]
        except (OSError, VerificationError) as caught:
            error = str(caught)

    report["status"] = "complete" if error is None else "failed"
    report["error"] = error
    atomic_write_json(report_path, report)
    if error is not None:
        print(f"error: {error}", file=sys.stderr)
        return 1

    cases = report["cases"]
    assert isinstance(cases, list)
    print(f"Recorded {len(cases)} VIC-II measurements at revision {revision}")
    print(f"Report: {report_path.relative_to(repo_root)}")
    return 0


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the revision-keyed PAL 6569 VIC-II breadth survey."
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow a diagnostic report from a dirty worktree",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    repo_root = Path(__file__).resolve().parent.parent
    try:
        revision, dirty = git_state(repo_root)
        require_dirty_policy(dirty, args.allow_dirty)
        testbench_root = required_directory(os.environ, TESTBENCH_ENV)
        rom_root = required_directory(os.environ, ROM_ENV)
        fixture_manifest = load_and_verify_assets(
            repo_root / "test-data/commodore/c64/vicii-vice-survey/assets-v1.json",
            testbench_root,
            rom_root,
        )
    except (OSError, subprocess.CalledProcessError, VerificationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    directory_name = report_directory_name(revision, dirty)
    output_root = repo_root / "target" / "accuracy" / "c64-vicii-survey" / directory_name
    lock_path = output_root.parent / f".{directory_name}.run.lock"
    try:
        lock = acquire_run_lock(lock_path)
    except (OSError, VerificationError) as error:
        print(f"error: cannot acquire survey run lock: {error}", file=sys.stderr)
        return 2
    try:
        return execute_survey(
            repo_root,
            output_root,
            revision,
            dirty,
            testbench_root,
            rom_root,
            fixture_manifest,
        )
    finally:
        release_run_lock(lock)


if __name__ == "__main__":
    raise SystemExit(main())

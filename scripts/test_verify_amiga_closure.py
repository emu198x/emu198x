#!/usr/bin/env python3
"""Focused tests for verify-amiga-closure.py."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("verify-amiga-closure.py")
REGRESSION_WRAPPER = SCRIPT.with_name("verify-amiga-regressions.sh")
SPEC = importlib.util.spec_from_file_location("verify_amiga_closure", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot import {SCRIPT}")
closure = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = closure
SPEC.loader.exec_module(closure)


class VerifyAmigaClosureTests(unittest.TestCase):
    def test_lane_selection_uses_canonical_order_and_rejects_unknown_ids(self) -> None:
        selected = closure.selected_lanes(["catalogue-ten", "test-kit-v1.12"])
        self.assertEqual(
            [lane.id for lane in selected],
            ["test-kit-v1.12", "catalogue-ten"],
        )
        with self.assertRaisesRegex(ValueError, "unknown lane ID"):
            closure.selected_lanes(["not-a-lane"])

    def test_closure_contract_has_strict_golden_and_external_input_lanes(self) -> None:
        self.assertEqual(closure.validate_commands(SCRIPT.parent.parent, closure.LANES), [])
        self.assertEqual(len(closure.LANES), 10)
        golden = closure.LANE_BY_ID["golden-matrix"]
        self.assertEqual(
            golden.argv,
            ("scripts/verify-amiga-golden-matrix.sh",),
        )
        self.assertEqual(
            [item.name for item in golden.required_environment],
            ["EMU198X_AMIGA_A1000_KICKSTART_DISK"],
        )
        self.assertEqual(
            [item.name for item in closure.LANE_BY_ID["test-kit-v1.12"].required_environment],
            ["EMU198X_AMIGA_TEST_KIT_ADF"],
        )
        self.assertEqual(
            [
                item.name
                for item in closure.LANE_BY_ID["test-kit-v1.21-ocs"].required_environment
            ],
            ["EMU198X_AMIGA_TEST_KIT_V121_ADF"],
        )
        self.assertEqual(
            closure.LANE_BY_ID["test-kit-v1.21-ocs"].validator,
            "test-kit-v1.21-ocs",
        )
        self.assertEqual(
            closure.LANE_BY_ID["test-kit-v1.21-aga"].validator,
            "test-kit-v1.21-aga",
        )
        self.assertEqual(
            [
                item.name
                for item in closure.LANE_BY_ID["catalogue-ten"].required_environment
            ],
            [
                "EMU198X_CATALOGUE_MEDIA_ROOT",
                "EMU198X_CATALOGUE_FIRMWARE_ROOT",
            ],
        )
        self.assertEqual(
            closure.LANE_BY_ID["programmable-hblank-write-timing"].argv,
            ("scripts/verify-amiga-programmable-hblank-write-timing.sh",),
        )
        self.assertEqual(
            closure.LANE_BY_ID["catalogue-ten"].argv,
            ("scripts/verify-amiga-catalogue.sh",),
        )
        self.assertEqual(
            closure.LANE_BY_ID["amiga-regressions"].argv,
            ("scripts/verify-amiga-regressions.sh",),
        )
        golden_wrapper = SCRIPT.with_name("verify-amiga-golden-matrix.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn("--lane golden-matrix", golden_wrapper)
        self.assertIn("EMU198X_REQUIRE_GOLDEN_ASSETS=1", golden_wrapper)
        catalogue_wrapper = SCRIPT.with_name("verify-amiga-catalogue.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn("--lane catalogue-ten", catalogue_wrapper)
        self.assertIn("cargo run --locked --release", catalogue_wrapper)
        wrapper = REGRESSION_WRAPPER.read_text(encoding="utf-8")
        for required_fragment in (
            "cargo test --locked --lib",
            "-p motorola-68000",
            "-p motorola-68040",
            "-p commodore-agnus-ocs",
            "-p commodore-denise-aga",
            "-p commodore-paula-8364",
            "-p commodore-gayle",
            "-p mos-cia-8520",
            "-p peripheral-commodore-amiga-floppy",
            "-p machine-commodore-amiga-a1200",
            "-p runtime-commodore-amiga",
            "--test arbitration",
            "--test disk_dma_arbitration",
            "--test dsk_write_back",
            "--test blitter_register_writes",
            "--test incremental_blitter",
            "--test dsk_writable_mount",
            "--test queries",
            "--test a1200_interrupt_snapshot",
            "-p emu198x-catalogue --test amiga_manifest",
        ):
            self.assertIn(required_fragment, wrapper)

    def test_catalogue_validation_requires_exact_reviewed_ids_and_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "catalogue.log"
            lines = []
            for entry_id in closure.EXPECTED_CATALOGUE_IDS:
                lines.append(f"[PASS] {entry_id} (reviewed title)\n")
                lines.append(f"[SNAP-PASS] {entry_id}\n")
            log.write_text("".join(lines), encoding="utf-8")
            result = closure.validate_catalogue_log(log)
            self.assertEqual(result["status"], "pass")
            self.assertEqual(result["actual_pass_markers"], 10)
            self.assertEqual(result["actual_snapshot_pass_markers"], 10)
            self.assertEqual(
                result["actual_pass_ids"], list(closure.EXPECTED_CATALOGUE_IDS)
            )

            wrong_lines = list(lines)
            wrong_lines[0] = "[PASS] unreviewed-entry (wrong fixture)\n"
            log.write_text("".join(wrong_lines), encoding="utf-8")
            result = closure.validate_catalogue_log(log)
            self.assertEqual(result["status"], "fail")
            self.assertFalse(result["pass_ids_exact_and_ordered"])

            reordered = list(lines)
            reordered[0], reordered[2] = reordered[2], reordered[0]
            log.write_text("".join(reordered), encoding="utf-8")
            result = closure.validate_catalogue_log(log)
            self.assertEqual(result["status"], "fail")
            self.assertFalse(result["pass_ids_exact_and_ordered"])

    def test_test_kit_validation_requires_exact_contract_markers_and_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "test-kit.log"
            for validator_id, expected in closure.EXPECTED_TEST_KIT_V121_MARKERS.items():
                lines = [f"test ignored ... {expected[0]}\n"]
                lines.extend(f"{marker}\n" for marker in expected[1:])
                log.write_text("".join(lines), encoding="utf-8")
                result = closure.validate_test_kit_v121_log(log, validator_id)
                self.assertEqual(result["status"], "pass")
                self.assertTrue(result["markers_exact_and_ordered"])
                self.assertEqual(result["actual_marker_count"], 6)

                reordered = list(lines)
                reordered[1], reordered[2] = reordered[2], reordered[1]
                log.write_text("".join(reordered), encoding="utf-8")
                result = closure.validate_test_kit_v121_log(log, validator_id)
                self.assertEqual(result["status"], "fail")
                self.assertFalse(result["markers_exact_and_ordered"])

                duplicated = list(lines)
                duplicated.append(lines[-1])
                log.write_text("".join(duplicated), encoding="utf-8")
                result = closure.validate_test_kit_v121_log(log, validator_id)
                self.assertEqual(result["status"], "fail")
                self.assertFalse(result["markers_unique"])

        with self.assertRaisesRegex(ValueError, "unknown Test Kit"):
            closure.validate_test_kit_v121_log(Path("unused"), "unknown")

    def test_redactor_removes_environment_path_values_and_descendants(self) -> None:
        private_root = "/private/reference/library"
        redact = closure.make_redactor(
            {
                "EMU198X_CATALOGUE_MEDIA_ROOT": private_root,
                "EMU198X_AMIGA_TEST_KIT_ADF": f"{private_root}/kit.adf",
            }
        )
        output = redact(
            f"read {private_root}/games/demo.adf and {private_root}/kit.adf"
        )
        self.assertNotIn(private_root, output)
        self.assertNotIn("kit.adf", output)
        self.assertIn("<redacted:EMU198X_AMIGA_TEST_KIT_ADF>", output)

    def test_environment_preflight_never_echoes_a_path_value(self) -> None:
        private_path = "/private/reference/does-not-exist.adf"
        errors = closure.validate_required_environment(
            [closure.LANE_BY_ID["test-kit-v1.12"]],
            {"EMU198X_AMIGA_TEST_KIT_ADF": private_path},
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("EMU198X_AMIGA_TEST_KIT_ADF", errors[0])
        self.assertNotIn(private_path, errors[0])

    def test_report_records_command_ids_but_not_argv_or_environment_values(self) -> None:
        report = closure.new_report("a" * 40)
        encoded = json.dumps(report)
        self.assertNotIn('"argv"', encoded)
        self.assertNotIn("/private/reference/library", encoded)
        self.assertEqual(report["revision"], "a" * 40)
        self.assertEqual(
            [lane["command_id"] for lane in report["lanes"]],
            [lane.id for lane in closure.LANES],
        )

    def test_disagreement_registry_has_only_terminal_campaign_classes(self) -> None:
        repo_root = SCRIPT.parent.parent
        closure.validate_registry(repo_root)
        self.assertEqual(
            tuple(row["id"] for row in closure.DISAGREEMENT_REGISTRY),
            closure.EXPECTED_DISAGREEMENT_IDS,
        )
        classifications = {
            row["classification"] for row in closure.DISAGREEMENT_REGISTRY
        }
        self.assertEqual(
            classifications,
            {"fixed", "scoped-out", "blocked-stronger-evidence"},
        )
        registry = {
            row["id"]: row["classification"]
            for row in closure.DISAGREEMENT_REGISTRY
        }
        self.assertEqual(
            registry["a1000-workbench-pointer-golden-baseline"], "fixed"
        )
        self.assertEqual(
            registry["a1000-workbench-free-memory-readout"], "scoped-out"
        )
        self.assertEqual(
            registry["denise-ocs-color-output-phase"],
            "blocked-stronger-evidence",
        )
        self.assertEqual(
            registry["aga-sprite-horizontal-output-phase"],
            "blocked-stronger-evidence",
        )

        missing_row = closure.DISAGREEMENT_REGISTRY[:-1]
        with self.assertRaisesRegex(RuntimeError, "ID set or order"):
            closure.validate_registry(repo_root, missing_row)

        missing_document = json.loads(json.dumps(closure.DISAGREEMENT_REGISTRY))
        missing_document[0]["documents"] = ["knowledge/decisions/not-present.md"]
        with self.assertRaisesRegex(RuntimeError, "document is missing"):
            closure.validate_registry(repo_root, missing_document)

    def test_revision_run_lock_excludes_concurrent_invocations(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            lock_path = Path(temporary) / "revision.run.lock"
            first = closure.acquire_run_lock(lock_path)
            try:
                with self.assertRaisesRegex(RuntimeError, "another Amiga closure"):
                    closure.acquire_run_lock(lock_path)
            finally:
                closure.release_run_lock(first)

            second = closure.acquire_run_lock(lock_path)
            closure.release_run_lock(second)

    def test_atomic_json_write_leaves_complete_document(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report.json"
            closure.atomic_write_json(report, {"revision": "first", "lanes": []})
            closure.atomic_write_json(report, {"revision": "second", "lanes": [1]})
            self.assertEqual(
                json.loads(report.read_text(encoding="utf-8")),
                {"revision": "second", "lanes": [1]},
            )
            self.assertEqual(list(report.parent.glob(".report.json.*.tmp")), [])

    def test_passing_archive_is_complete_atomic_and_immutable(self) -> None:
        revision = "b" * 40
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            output = repo / "target" / "accuracy" / "amiga-closure" / revision
            report = closure.new_report(revision)
            report["status"] = "pass"
            report["dirty"] = False
            for index, lane in enumerate(report["lanes"]):
                relative_log = Path("logs") / f"lane-{index}.log"
                log = output / relative_log
                log.parent.mkdir(parents=True, exist_ok=True)
                contract_lane = closure.LANE_BY_ID[lane["command_id"]]
                if contract_lane.validator == "catalogue-ten":
                    lines = []
                    for entry_id in closure.EXPECTED_CATALOGUE_IDS:
                        lines.append(f"[PASS] {entry_id} (reviewed title)\n")
                        lines.append(f"[SNAP-PASS] {entry_id}\n")
                    log.write_text("".join(lines), encoding="utf-8")
                elif contract_lane.validator in closure.EXPECTED_TEST_KIT_V121_MARKERS:
                    markers = closure.EXPECTED_TEST_KIT_V121_MARKERS[
                        contract_lane.validator
                    ]
                    log.write_text("\n".join(markers) + "\n", encoding="utf-8")
                else:
                    log.write_text(f"redacted lane {index}\n", encoding="utf-8")
                lane["status"] = "pass"
                attempt = {
                    "command_id": lane["command_id"],
                    "revision": revision,
                    "dirty": False,
                    "status": "pass",
                    "exit_code": 0,
                    "log": relative_log.as_posix(),
                    "log_sha256": closure.sha256_file(log),
                }
                if contract_lane.validator is not None:
                    attempt["validation"] = closure.validate_lane_log(
                        contract_lane, log
                    )
                lane["attempts"] = [attempt]
            closure.atomic_write_json(output / closure.REPORT_FILENAME, report)

            missing_hash = json.loads(json.dumps(report))
            missing_hash["lanes"][0]["attempts"][0]["log_sha256"] = None
            closure.atomic_write_json(output / closure.REPORT_FILENAME, missing_hash)
            with self.assertRaisesRegex(RuntimeError, "missing log SHA-256"):
                closure.archive_passing_report(repo, output, missing_hash)

            nonzero_exit = json.loads(json.dumps(report))
            nonzero_exit["lanes"][0]["attempts"][0]["exit_code"] = 1
            closure.atomic_write_json(output / closure.REPORT_FILENAME, nonzero_exit)
            with self.assertRaisesRegex(RuntimeError, "non-zero or missing exit code"):
                closure.archive_passing_report(repo, output, nonzero_exit)

            missing_validation = json.loads(json.dumps(report))
            test_kit_lane = next(
                lane
                for lane in missing_validation["lanes"]
                if lane["command_id"] == "test-kit-v1.21-ocs"
            )
            del test_kit_lane["attempts"][0]["validation"]
            closure.atomic_write_json(
                output / closure.REPORT_FILENAME, missing_validation
            )
            with self.assertRaisesRegex(
                RuntimeError,
                "test-kit-v1.21-ocs latest attempt has no passing marker validation",
            ):
                closure.archive_passing_report(repo, output, missing_validation)

            forged_validation = json.loads(json.dumps(report))
            forged_lane = next(
                lane
                for lane in forged_validation["lanes"]
                if lane["command_id"] == "test-kit-v1.21-ocs"
            )
            forged_attempt = forged_lane["attempts"][0]
            forged_log = output / forged_attempt["log"]
            valid_log_bytes = forged_log.read_bytes()
            forged_log.write_text("no case markers\n", encoding="utf-8")
            forged_attempt["log_sha256"] = closure.sha256_file(forged_log)
            forged_attempt["validation"] = {"status": "pass"}
            closure.atomic_write_json(
                output / closure.REPORT_FILENAME, forged_validation
            )
            with self.assertRaisesRegex(
                RuntimeError,
                "stored marker validation differs from the hashed latest log",
            ):
                closure.archive_passing_report(repo, output, forged_validation)
            forged_log.write_bytes(valid_log_bytes)

            dirty_report = json.loads(json.dumps(report))
            dirty_report["dirty"] = True
            closure.atomic_write_json(output / closure.REPORT_FILENAME, dirty_report)
            with self.assertRaisesRegex(RuntimeError, "dirty-worktree"):
                closure.archive_passing_report(repo, output, dirty_report)
            closure.atomic_write_json(output / closure.REPORT_FILENAME, report)

            archive_root = (
                repo / "test-data" / "commodore" / "amiga" / "closure-reports"
            )
            lock_path = archive_root / f".{revision}.archive.lock"
            lock_path.parent.mkdir(parents=True, exist_ok=True)
            lock_path.write_text("active publisher\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "publication is already active"):
                closure.archive_passing_report(repo, output, report)
            self.assertEqual(
                lock_path.read_text(encoding="utf-8"), "active publisher\n"
            )
            lock_path.unlink()

            destination = closure.archive_passing_report(repo, output, report)
            self.assertEqual(destination.name, revision)
            self.assertEqual(
                json.loads(
                    (destination / closure.REPORT_FILENAME).read_text(encoding="utf-8")
                ),
                report,
            )
            self.assertEqual(len(list((destination / "logs").glob("*.log"))), 10)
            with self.assertRaisesRegex(FileExistsError, "already exists"):
                closure.archive_passing_report(repo, output, report)
            self.assertEqual(
                list(destination.parent.glob(f".{revision}.*.tmp")), []
            )
            self.assertEqual(
                list(destination.parent.glob(f".{revision}.archive.lock")), []
            )


if __name__ == "__main__":
    unittest.main()

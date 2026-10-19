# SPDX-License-Identifier: GPL-2.0-or-later
"""Readable finite native-state gate, keeping hardware qualification separate."""

from __future__ import annotations

import html
import json
from pathlib import Path

import verify

HERE = Path(__file__).resolve().parent


def main() -> None:
    v = verify.audit(HERE / "runs")
    if (v["fixtures"], v["checkpoints_per_repeat"], v["fresh_native_restores"]) != (
        13,
        180,
        360,
    ):
        raise ValueError("report requires the complete verified native matrix")
    d = json.loads((HERE / "runs/campaign.json").read_text())
    models = verify.fixtures()
    rows = []
    for name, f in models.items():
        points = verify.keys(name, f)
        rows.append(
            [
                name,
                f"{f['command_n']} / {f['recorded_n']}",
                str(f["dtl"]),
                ",".join(map(str, f["ids"])),
                ",".join(map(str, f["lengths"])),
                str(f["eot"]),
                str(sum(p[1] == "request-low" for p in points)),
                str(sum(p[1] == "request-high" for p in points)),
                str(sum(p[1] == "result" for p in points)),
            ]
        )
    summary = "13 fixtures, 180 native checkpoints per fresh run, two runs: 360 successful restores. Wrong restored-continuation expectation rejected at the intended assertion."
    headers = [
        "Fixture",
        "Command / recorded N",
        "DTL",
        "IDs",
        "Stored bytes",
        "EOT",
        "RQM low",
        "RQM high",
        "Result checkpoints",
    ]
    text = [
        "# Read Track native field checkpoints",
        "",
        summary,
        "",
        "**Native-state preservation passes for this finite matrix. All 15 FDC operations remain partial.** This is not a new Spectrum configuration qualification or a physical timing result.",
        "",
        "## What passed",
        "",
        "- Actual native v6 state is saved and restored into a fresh blank +3 runtime with no disk. Every restore is byte-identical to the checkpoint. All remaining payload and result bytes match uninterrupted execution, as do final native state, idle status and a subsequent Sense Drive Status command.",
        "- The mounted live image deliberately differs from cached source before Read Track begins. Short/long fields keep their lengths; an empty-field fixture has an unvisited canary. The missing-entry fixture removes the live track while retaining a valid cached source. Each checkpoint checks original source bytes and the complete serialized live image, so cached-source replay cannot substitute for restoration.",
        "- Payload checkpoints cover first/interior/final-byte and applicable field/wrap boundaries at RQM low and high. Every fixture has result checkpoints after 0/1/3/6 result bytes. Missing entry has immediate result-only checkpoints, not fictitious payload assertions.",
        "- Two independent fresh test processes produce matching observations, fixtures and checkpoint bytes. The Rust test asserts exactly 13 cases and 180 checkpoints. The offline auditor independently enumerates those keys and compares raw logs, byte vectors, identities and repetitions.",
        "- A wrong expected byte is deliberately introduced into the restored continuation. The same compiled test exits 101 with the intended restored-continuation assertion; this is negative-control success, not a passing execution. Nineteen auditor controls also reject missing/duplicate/zero checkpoints, wrong vectors/sizes, corrupt native states after rehashing, lost recovery and incorrect failure reasons.",
        "",
        "## Configuration and provenance",
        "",
        f"Source-built Emu crate version 0.38.3, base {d['source']['base_commit']} plus the exact archived snapshot.rs test edits. Compiler/executable/recipe/source and all asset hashes are in the campaign and manifests. Rust/Cargo and Python executable identities are retained. This uses SpectrumPlus3Runtime::blank(), the existing EDSK codec and existing direct FDC tick helpers. There is no firmware input: the machine has zero-filled ROM banks. Retained native states therefore contain blank machine memory, not proprietary firmware.",
        "",
        "The live edit is fixture construction through the existing controller insertion API. It is not an emulated Write Data result. Direct FDC service is not a CPU/ROM disk-loading run. Baseline expectations describe current model continuation; they do not adjudicate unresolved DTL/field-size reference differences or establish silicon deadlines. No production behaviour, native schema, dependency or support surface changed.",
        "",
        "Independent reference replay of this private native-v6 format is not applicable. The earlier fresh MAME/Fuse/controller studies in PR #1879 remain separate evidence; they were not rerun or promoted into native-state qualification here. Native preservation does not qualify magnetic gaps, flux, drive wiring, paging or unsupported hardware variants.",
        "",
        "## Verification",
        "",
        "The matrix passed twice and its wrong-expectation control failed as intended. The auditor passed under optimized Python and all 19 adversarial controls passed. Scoped runtime/controller regression: 477 libtest-reported passes, zero failures, 62 explicitly ignored tests; ignores receive no qualification credit. Scoped all-target Clippy, Rust format and packet Ruff checks pass. Supported build/full-workspace checks remain CI gates.",
        "",
        "```sh",
        "python3.14 -B -O docs/plans/2026-10-19-read-track-native-fields/verify.py",
        "python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-native-fields -p test_verify.py -v",
        "```",
        "",
        "For fresh execution in a full source checkout, move the packet’s previous runs/ directory aside, then run `python3.14 -u docs/plans/2026-10-19-read-track-native-fields/run.py`. Rust/Cargo/rustup and Python 3.14 are required; no ROMs or reference emulator installation are needed. The runner refuses an existing output directory. Evidence includes authored EDSK source, serialized live fixtures, all blank-runtime checkpoint files, logs and exact capture source. No source fixture path is overwritten.",
        "",
        "## Remaining closure gates",
        "",
        "- Review/CI and merge of this native-state gate and comparison PR #1879.",
        "- Applicable released guest execution for boundary fixtures. The previously released basic ROM-loader diagnostic does not exercise this matrix.",
        "- Adjudication of the retained size/DTL/opcode/reference disagreements and capability-specific hardware/clock/revision evidence (#1831 and #1876). No upstream Fuse defect report follows from disagreement alone.",
        "",
        "## Every fixture",
        "",
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * len(headers)) + " |",
    ]
    text += ["| " + " | ".join(row) + " |" for row in rows]
    (HERE / "README.md").write_text("\n".join(text) + "\n")
    page = (
        """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Read Track native-state gate</title><style>body{font:17px system-ui;color:#222;background:#fafaf7;margin:32px}h1{font-size:32px}.cards{display:flex;flex-wrap:wrap;gap:16px}.card{border:1px solid #999;max-width:310px;padding:16px}.done{border-top:6px solid #31754d}.open{border-top:6px solid #b87519}p{max-width:1000px}table{font-size:13px;border-collapse:collapse;width:100%}th,td{text-align:left;padding:9px;border-bottom:1px solid #ccc}</style><h1>Read Track: native-state gate passes</h1><p>"""
        + html.escape(summary)
        + """</p><div class="cards"><div class="card done"><b>Done: finite native preservation</b><p>New sizes/lengths, reversed/duplicate IDs, EOT wrapping and missing entry. Exact restore, continuation, live media and recovery checked.</p></div><div class="card open"><b>Next: released guest boundary cases</b><p>Move selected boundary fixtures through real guest execution. Keep source-only state tests separate from released ROM-loader evidence.</p></div><div class="card open"><b>Still open: hardware evidence</b><p>Resolve applicable revision/clock/timing and retained reference disagreements. These are not settled by serialization tests.</p></div></div><p><b>All 15 operations remain partial.</b> Review/CI is pending. No effort percentage or broader qualification is inferred. <a href="README.md">Full report</a> · <a href="verification.json">Audit result</a></p><table><thead><tr>"""
        + "".join("<th>" + html.escape(s) + "</th>" for s in headers)
        + """</tr></thead><tbody>"""
        + "".join(
            "<tr>" + "".join("<td>" + html.escape(s) + "</td>" for s in row) + "</tr>"
            for row in rows
        )
        + """</tbody></table></html>"""
    )
    (HERE / "report.html").write_text(page)
    (HERE / "report.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "qualified": False,
                "summary": summary,
                "columns": headers,
                "rows": rows,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()

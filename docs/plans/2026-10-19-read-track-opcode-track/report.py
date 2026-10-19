# SPDX-License-Identifier: GPL-2.0-or-later
"""Small visual closure map backed by the frozen observations."""

from __future__ import annotations

import html
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main() -> None:
    d = json.loads((HERE / "runs/campaign.json").read_text())
    v = json.loads((HERE / "verification.json").read_text())
    rows = {(r["engine"], r["case"]): r for r in d["observations"] if r["repeat"] == 1}
    table = []
    for c in d["cases"]:
        row = [c["name"]]
        for engine in ("emu", "mame", "fuse"):
            r = rows[engine, c["name"]]
            if r["status"] == "not_applicable":
                row.append("N/A: container-entry condition")
                continue
            o = r["observed"]
            row.append(
                f"{o['command_bytes_sent']} command / {len(o['data'])} payload / "
                + " ".join(f"{b:02X}" for b in o["result"])
            )
        table.append(row)
    summary = f"{v['cases']} cases repeated twice: {v['accepted_command_captures']} complete accepted-command captures, {v['early_rejection_captures']} complete early-rejection captures, {v['not_applicable']} explicitly inapplicable reference rows."
    headers = ["Case", "Emu", "MAME", "Fuse"]
    text = [
        "# Read Track missing entry and opcode contrasts",
        "",
        summary,
        "",
        "**All 15 FDC operations remain partial.** This closes a finite observation gap. It does not qualify a Spectrum configuration, multi-track/skip operation or physical timing.",
        "",
        "## What now works",
        "",
        "- A present sector image with no track entry is exercised separately from a present empty track and from absent media. Emu returns seven bytes, drains to idle and accepts the next command. Its media remain present and unchanged. No raw-media equivalence is asserted for this container condition.",
        "- The same [1,2] and [2,1] physical fields, distinct payloads and empty raw track are exercised with opcodes 42, 62, C2 and E2. Each accepted command has all nine command bytes, its complete payload, all seven result bytes and subsequent recovery checked.",
        "- Fuse rejects C2/E2 immediately with one invalid-command byte 80. The host sends only the opcode and never feeds the eight unsent parameters into subsequent commands. This rejection has zero payload and real next-command recovery; it is not a timed-out seven-byte transfer.",
        "- Emu and installed MAME accept all four opcode variants and remain on the represented single side. This records observed compatibility only. It cannot establish supported multi-track or skip functionality.",
        "- Independent bit-serial raw CRC and independently decoded MFI clock/byte audits are reused. Commands, environment, source/recipe/executable identities, firmware hashes, full observations, repeatability and time bounds are retained without ROM bytes.",
        "",
        "## Evidence and limits",
        "",
        "Local verification: 143 controller tests pass with zero failures/ignores, and 18 auditor/prerequisite controls pass under optimized Python. Clippy, Rust format and packet Ruff checks pass. Full workspace and supported-build checks remain CI gates; no pass is inferred from a queued job.",
        "",
        "Fresh source Emu at 7222f23075ccec1db5ecb431421ef72139640e4f plus the archived probe edits (crate version 0.38.3); unchanged Fuse source 0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2 with the existing bounded host; installed MAME 0.289 with unknown source commit. Inspected MAME source is recorded separately. The MAME machine is the PAL English +3 with internal single-sided drive and four local English v4.0 firmware pages, identified by ordered hashes. The Emu/Fuse controller hosts use no firmware. MAME uses direct Lua port operations after a RAM idle loop, not ordinary ROM disk loading. `qualified=false`.",
        "",
        "NEC uPD765 December 1978 printed p8 fixes MT=0 but shows SK as variable in the command table; p14 says multi-track and skip operations are not allowed. Those observations do not prove whether a particular silicon revision rejects or ignores every combination. Fuse and Emu/MAME differ on acceptance; no upstream defect report or speculative production fix follows.",
        "",
        "Expected vectors in expectations.json pin these captured observations for regression and integrity. They are not physical-hardware oracles. The raw payload/order expectations are separately computed from authored sector order. A preparation capture used an inaccurate fixed-SK label; it is retained only as preparation, not credited as this matrix. The final capture uses corrected labels and exact archived sources.",
        "",
        "The prior 39-case size/DTL packet remains unchanged. Its DTL, command-vs-recorded size, field-length and EOT=0 differences remain unresolved. Applicable interruption/native-state coverage and fresh released guest execution for boundary cases remain separate, as does #1831 timing and #1876 drive wiring.",
        "",
        "## Next finite gate",
        "",
        "Current snapshot.rs already defines tests for N2 clean/CRC reads, two-sector boundaries, RQM high/low, overruns and result prefixes. Extend that existing harness to N0/N5 and the newly exposed short/empty/long represented fields, duplicate/reversed IDs and EOT wrapping. Compare uninterrupted vs restored continuation, result bytes and immutable media. Do not repeat the existing N2 gate or claim exact physical deadlines. The bounded follow-up is #1880.",
        "",
        "## Reproduce",
        "",
        "```sh",
        "python3.14 -B -O docs/plans/2026-10-19-read-track-opcode-track/verify.py",
        'python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-opcode-track -p "test_*.py" -v',
        "```",
        "",
        "Fresh capture: in the complete source checkout, move the packet’s previous runs/ directory aside and run `python3.14 -u docs/plans/2026-10-19-read-track-opcode-track/run.py`. It reuses the preceding runner, whose identity and source are included with the exact capture recipe. Requires existing local firmware, vendored clean Fuse sources, Clang, Rust and installed MAME 0.289. An existing output directory is rejected. The missing-firmware test uses an empty temporary home and invokes no emulator.",
        "",
        "## Every case",
        "",
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(["---"] * 4) + " |",
    ]
    text += ["| " + " | ".join(row) + " |" for row in table]
    (HERE / "README.md").write_text("\n".join(text) + "\n")
    cells = "".join(
        "<tr>" + "".join("<td>" + html.escape(s) + "</td>" for s in row) + "</tr>"
        for row in table
    )
    page = (
        """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Read Track remaining gates</title><style>body{font:17px system-ui;color:#222;background:#fafaf7;margin:32px}h1{font-size:32px}.cards{display:flex;flex-wrap:wrap;gap:16px}.card{border:1px solid #999;padding:16px;max-width:310px}.done{border-top:6px solid #31754d}.open{border-top:6px solid #b87519}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;border-bottom:1px solid #ccc;padding:10px}p{max-width:1000px}</style><h1>Read Track: two observation gaps covered</h1><p>"""
        + html.escape(summary)
        + """</p><div class="cards"><div class="card done"><b>Done: finite observations</b><p>Size/DTL packet, missing container track, physical order and command acceptance/rejection. Disagreements remain visible.</p></div><div class="card open"><b>Next: <a href="https://github.com/emu198x/emu198x/issues/1880">interrupted state #1880</a></b><p>Extend existing N2 checkpoint coverage to N0/N5, short/empty/long fields, duplicate/reversed IDs and EOT wrapping.</p></div><div class="card open"><b>Still needed for closure</b><p>Applicable revision/hardware evidence for disputed behaviour and timing; released guest coverage of boundary cases.</p></div></div><p><b>All 15 operations remain partial.</b> No effort percentage or broader machine qualification is inferred. <a href="README.md">Full report</a> · <a href="verification.json">Audit result</a></p><p>Cells show command bytes sent / payload bytes / actual result bytes. A one-byte 80 result is early rejection; its unsent parameters are never submitted.</p><table><thead><tr>"""
        + "".join("<th>" + html.escape(s) + "</th>" for s in headers)
        + """</tr></thead><tbody>"""
        + cells
        + """</tbody></table></html>"""
    )
    (HERE / "report.html").write_text(page)
    (HERE / "report.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "qualified": False,
                "summary": summary,
                "rows": table,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()

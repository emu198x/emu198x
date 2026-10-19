# SPDX-License-Identifier: GPL-2.0-or-later
"""Render the finite study without turning repeatable observations into truth."""

from __future__ import annotations

import html
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main() -> None:
    campaign = json.loads((HERE / "runs/campaign.json").read_text())
    audit = json.loads((HERE / "verification.json").read_text())
    rows = {
        (r["engine"], r["case"]): r
        for r in campaign["observations"]
        if r["repeat"] == 1
    }

    def cell(row: dict) -> str:
        if row["status"] != "pass":
            return row["status"]
        o = row["observed"]
        return f"{len(o['data'])} B / " + " ".join(f"{b:02X}" for b in o["result"])

    table = []
    matches = 0
    for c in campaign["cases"]:
        er = rows["emu", c["name"]]
        mr = rows["mame", c["name"]]
        if mr["status"] == "not_applicable":
            outcome = "Safety/absence control only"
        elif er["status"] == "pass" and mr["status"] == "pass":
            equal = all(
                er["observed"][f] == mr["observed"][f] for f in ("data", "result")
            )
            outcome = (
                "Emu/MAME agree"
                if equal
                else "Different inputs or behaviour; unresolved"
            )
            matches += int(equal)
        else:
            outcome = "Capture failed; no agreement credit"
        table.append(
            [c["name"], cell(er), cell(mr), cell(rows["fuse", c["name"]]), outcome]
        )
    summary = f"{audit['case_count']} named cases; {audit['successful_observations']} successful captures; {len(audit['failures'])} bounded capture failures; {audit['not_applicable']} explicitly inapplicable rows. {matches} Emu/MAME case pairs agree on every payload and result byte, repeated twice."
    header = [
        "Case",
        "Emu: payload / ST0 ST1 ST2 C H R N",
        "MAME: payload / result",
        "Fuse: payload / result",
        "Interpretation",
    ]
    lines = [
        "# Read Track size and field boundary study",
        "",
        "**All 15 FDC operations remain partial.** This study narrows Read Track; it does not qualify a machine or establish silicon timing.",
        "",
        summary,
        "",
        "Fresh Emu source d92b7ca3828efc438f335a2dbdb337c80cf7f346 plus the archived test harness; installed MAME 0.289 (source identity unknown); unmodified Fuse source 0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2 with a bounded register/event host. Emu crate version 0.38.3 is a source build, not the released executable. MAME uses the PAL English +3 and the four locally supplied English v4.0 ROM pages, whose hashes are in the campaign. Register hosts bypass normal guest loading; qualified=false.",
        "",
        "## What the checks establish",
        "",
        "- Valid 250 kbit/s MFM raw fields N0–N5 fit a 6250-byte revolution. Both repetitions drain all seven result bytes, verify idle and issue a subsequent Sense Drive Status command. Source media remain unchanged.",
        "- N0 DTL=0,1,127,128,255 and a nonzero-N contrast are distinct cases. Commanded and recorded N remain separate inputs. Duplicate IDs have distinct payloads; both physical orders and EOT below/equal/above physical field count are retained.",
        "- The raw fixture has its CRC after the authored actual data length. Independent bit-serial CRC and independently decoded MFI clocks/bytes are checked. Emu receives a sector representation; zero padding of a short/empty representation is not an on-disk field, and is not evidence for a missing data address mark.",
        "- N6/N7 cannot fit this raw revolution, and N8/255 are invalid size-code controls. These and absent media have fresh Emu observations only; reference rows say not applicable, not pass.",
        "- Capture pass means bounded payload/result drain and recovery succeeded. It does not mean the observed bytes match hardware. Expectations pin captured observations for evidence integrity; they are not independently established silicon oracles. Nominal Emu payload/order checks are separately asserted.",
        "",
        "## What remains open",
        "",
        "- DTL: Fuse transfers 1/127 bytes for those N0 settings; Emu and MAME transfer 128. The NEC December 1978 sheet p.12 describes Read Data DTL and p.14 describes Read Track separately. This disagreement needs command- and revision-specific evidence before a behavioural fix.",
        "- Command/recorded size and actual raw-field length mismatches expose CRC/gap bytes in raw emulators. Sector-image padding cannot model that stream faithfully. Keep these as capability/input-boundary observations, not false equivalence.",
        "- Empty/unformatted track, physical CHRN mismatches, duplicate IDs and EOT edge cases remain explicit observations in the table. EOT=0 fails the bounded Fuse drain; its assertion log is retained. No upstream defect claim is made.",
        "- A missing physical track within an otherwise present image is not separately represented by this matrix. MT/SK opcode contrasts, applicable native-state interruption checkpoints and fresh released guest execution for the new boundaries remain separate work. Physical search/rotation/service deadlines remain #1831; drive signal wiring remains #1876.",
        "- No production controller code, dependencies, snapshot schemas or release boundaries change in this packet. A passing default does not qualify the family.",
        "",
        "## Reproduce",
        "",
        "```sh",
        "python3.14 -B -O docs/plans/2026-10-19-read-track-field-boundaries/verify.py",
        'python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-field-boundaries -p "test_*.py" -v',
        "```",
        "",
        "For fresh capture, remove or move only this packet’s previous `runs/` directory first, then run `python3.14 -u docs/plans/2026-10-19-read-track-field-boundaries/run.py`. It requires the family’s vendored Fuse source, local +3 firmware, Rust, Clang and installed MAME 0.289. The runner refuses to reuse an existing result directory. A missing firmware home is tested without moving any real firmware. `capture-source.tar.gz` retains the exact pre-format recipe used by this capture; current scripts include subsequent lint cleanup and automatic capture-source retention for future runs. `evidence.tar.gz` retains commands, environment, raw inputs, complete observations and failure logs. No ROM or snapshot is included.",
        "",
        "## Every case",
        "",
        "| " + " | ".join(header) + " |",
        "| " + " | ".join(["---"] * len(header)) + " |",
    ]
    lines += ["| " + " | ".join(row) + " |" for row in table]
    (HERE / "README.md").write_text("\n".join(lines) + "\n")
    th = "".join("<th>" + html.escape(x) + "</th>" for x in header)
    trs = "".join(
        "<tr>" + "".join("<td>" + html.escape(x) + "</td>" for x in row) + "</tr>"
        for row in table
    )
    page = (
        """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Read Track: remaining work</title><style>body{font:17px system-ui;margin:32px;color:#222;background:#fafaf7}h1{font-size:32px} .cards{display:flex;gap:16px;flex-wrap:wrap}.card{padding:18px;border:1px solid #aaa;max-width:320px}.good{border-top:6px solid #31754d}.open{border-top:6px solid #b87519}table{border-collapse:collapse;font-size:13px;width:100%}th,td{text-align:left;padding:9px;border-bottom:1px solid #ccc}input{font:inherit;padding:8px;margin:20px 0}code{font-size:13px}p{max-width:1000px}</style><h1>Read Track: closer, still partial</h1><p>"""
        + html.escape(summary)
        + """</p><div class="cards"><div class="card good"><b>Completed here</b><p>Repeated size, DTL, physical order and field-boundary observations. Independent raw CRC/clock audit and negative controls.</p></div><div class="card open"><b>Next implementation gate</b><p>Missing-track and opcode contrasts, then applicable native-state interruption checks. Retain size/DTL disagreements.</p></div><div class="card open"><b>Still prevents operation closure</b><p>Applicable hardware/revision evidence for disputed behaviour and precise timing; released guest coverage of the new boundaries.</p></div></div><p><b>0 of 15 operations fully qualified.</b> No effort percentage is inferred. This is a direct-register source/reference study; it adds no machine qualification. <a href="README.md">Full report</a> · <a href="verification.json">Audit result</a></p><label>Filter cases <input id="filter" placeholder="e.g. dtl, eot, safety"></label><table><thead><tr>"""
        + th
        + """</tr></thead><tbody>"""
        + trs
        + """</tbody></table><script>document.querySelector('#filter').addEventListener('input',e=>{for(const r of document.querySelectorAll('tbody tr'))r.hidden=!r.textContent.toLowerCase().includes(e.target.value.toLowerCase())})</script></html>"""
    )
    (HERE / "report.html").write_text(page)
    (HERE / "report.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "qualified": False,
                "summary": summary,
                "columns": header,
                "rows": table,
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()

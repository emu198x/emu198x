# A finite path to closing an FDC operation

**Read Track is the first candidate, but is still partial.** The raw-input CRC
gate is complete; the whole operation is not. All 15 documented operations
remain partial. We must close functional gaps with bounded matrices, then name
the hardware and execution evidence still missing. No percentage is claimed:
case counts are not a measure of all possible hardware behaviour.

This assessment reads controller source at
`523d37dd98e4ae168d341b34decc9afc654a7515`, its current integration tests and
#1831/#1860. The Read ID correction in #1867 is separate work. Historical .36
source inspection is not a description of today's code.

| Gate towards Read Track closure | Current position | Concrete exit |
| --- | --- | --- |
| Represented physical order, requested field count, drained results and next command | Covered for bounded one/two-sector N2 cases | Retain existing `read_track.rs` / `read_track_drained.rs` and frozen references as regressions |
| Clean / first-last ID and data CRC | Complete for the 13-case matrix | This packet independently audits raw input and compares all payload/result bytes twice; retain Fuse disagreements |
| Size, DTL and represented field boundaries | Next bounded work | Compare valid N0–5 fields and N0 DTL choices, short/long represented data, absent fields, ID mismatches/duplicates, EOT shorter/longer than track; freeze expected payload/status/CHRN and recovery per case from primary plus independent reference |
| Interruption and persistence for those cases | Partly covered; not complete | Check preamble, request low/high, current/final byte, error/result and selected-media-change boundaries; compare uninterrupted versus native save/restore results and immutable source bytes |
| Released guest exercises these features | Not run by this study | A redistributable diagnostic loads through ROM, issues actual Read Track cases, checks payload/status and reports an execution-only signature; repeat on exact released binary with wrong-expectation and damaged-media controls |
| Applicable physical timing and revision | Blocked by missing evidence | Identify controller revision/clock and obtain applicable request/service/overrun, search/index/head-load and completion timing evidence; implement and test before/at/after boundaries (#1831) |

The size study must distinguish valid media from synthetic input safety. At
250 kbit/s the current raw fixture has 6250 bytes per revolution: N6/N7 fields
cannot simply be placed on that track. Check larger declared sizes as parser/
controller safety inputs without claiming physically valid +3 disks. Test
other density/drive combinations only when the machine configuration supports
them. DSK/EDSK cannot represent arbitrary raw gaps, missing marks or every CRC
combination; use existing raw reference input and record an explicit limitation
where the production sector model cannot express a case. Do not manufacture a
pass from filler bytes or pretend all format flags are independent.

Current `prepare_read_sector` uses commanded N for Read Track and ignores DTL;
the inspected MAME Read Track path also uses N. That is a **question to test**,
not a confirmed bug. `read_progress.rs` already covers some interruption/status
lifetime and media-change cases; `read_pacing.rs` covers modelled request timing.
Those are reusable gates, not evidence for an unmeasured silicon deadline.

## Other operations

| Operation group | Why it cannot close yet | Smallest useful work |
| --- | --- | --- |
| Read ID | ID CRC now has a red/green correction (#1867); ID progression is call-driven, not rotation/index timed | Ship the verified correction; then test absent/duplicate/ID-error progression and native-state boundaries. Leave physical search/ready/revision work open |
| Sense Drive Status | Current TS and RY both derive from disk presence, rather than independent drive capability and pins | Reproduce exact +3 ST3 wiring/capabilities from primary and reference; decide the smallest drive-state correction before claiming completion |
| Sense Interrupt Status | No-pending one-byte result is fixed; pending/ready/reset event scope is not complete | Enumerate accepted events and drain/ordering/restore cases, including multiple drives, against applicable controller wiring |
| Seek / Recalibrate / Specify | Fixed SEEK_TICKS and immediate PCN update; Specify accepts timing/ND parameters without applying them | Evidence-backed mechanical/ready/interrupt model and tests. These are implementation gaps, not just missing reports |
| Read/Write/Deleted/Scan/Format | Implemented sector-model commands have bounded tests and some guest evidence; timing, search and opcode-specific edges remain | Reuse the same explicit boundary and persistence gates, retaining Scan #1813, no-TC Write #1814 and read timing #1831 separately |

There are three distinct claims: an implemented sector-model capability, a
feature actually exercised by a released guest, and clock/revision-specific
hardware qualification. Completing the first two would be valuable and should
close their scoped implementation/evidence tickets. It must not silently change
the definition of whole-operation completion or mark the physical gate passed.

The next implementation task is the **Read Track size/DTL/field boundary matrix**.
It has finite inputs and inspectable exits. Any confirmed bug goes into its
smallest owning issue/PR with a failing-before/passing-after check; unrelated
reference disagreements remain observations. We can finish that gate while the
specific physical timing prerequisite is unavailable.

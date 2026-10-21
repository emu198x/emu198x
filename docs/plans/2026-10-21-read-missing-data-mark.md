# Read commands encountering a missing data address mark

Lane: Spectrum launch-hardening. Inspection base: `569b0b1f0448aaace5da97dc8ed0ba897646ee5d`.

Investigate whether the sector model transfers data despite represented ST2.MD. This is a hypothesis until a fail-first run. No new API, dependency or native schema.

1. Author valid one-track EDSK with a matching readable ID, canonical missing-DAM status ST1.MA/ST2.MD, zero or 256 stored bytes, and a following good sector. Parse through the existing format decoder, using existing runtime test dependencies and +3 controller composition.
2. Run bounded existing-register Read Data/Read Deleted Data checks before production edits: zero payload, abnormal MA/MD result, all seven result bytes, next-command recovery, logical alias echo and unchanged live image. Include SK so missing DAM cannot be mistaken for an ordinary deleted-mark mismatch.
3. If demonstrated, correct only shared Data/Deleted preparation after ID-CRC validation and before mark mismatch/payload access. Keep Read Track outside this fix; physical search/index/timing and Read Track semantics remain separate claims.
4. Run the affected regression and full controller/format/common/runtime suites plus strict Clippy/format with explicit Rust 1.99.0. Retain red and green logs, exact commands/bounds/source identities. Open an owning issue/PR; do not merge or publish releases.

Primary: NEC uPD765A/B datasheet printed 5-12, ST1.MA and ST2.MD definitions (shared `reference/by-topic/fdc-765/nec-upd765ab-datasheet.*`, original PDF hash in sidecar). Missing DAM/DDAM sets both status bits. MAME `upd765.cpp` lines 1109–1114 sets MA/MD; lines 2017–2021 aborts Read Data before a payload FIFO. Fuse `upd_fdc.c::read_datamark/start_read_data` also aborts before payload, but only sets MD in the inspected implementation: that status disagreement is retained, not treated as authoritative over primary text.

Reference format limitation: inspected MAME `dsk_dsk.cpp` lines 475–484 uses ST1.ND to omit data and ignores ST2.MD. Fuse `disk.c` lines 1833 onward builds ID/data marks from payload/CRC/CM and ignores MD. Therefore an MD-only EDSK reference run would not instantiate the intended missing-DAM fault. This is source inspection, not a fresh independent runtime comparison or an upstream defect submission.

## Demonstrated result

Both fail-first tests failed at the intended zero-payload assertion. With zero stored bytes for the bad field, Read Data delivered 512 host bytes (invented bad-field bytes plus the next good sector); Read Deleted delivered 256 invented bytes. The parser correctly retained canonical MA/MD and zero stored length. Shared preparation ignored MD and proceeded to payload fill/transfer.

The fix checks represented MD after the existing ID-CRC rejection, before SK/mark mismatch and payload access. It derives MA/MD and finishes through the existing result path. It adds no fields or interfaces and does not alter Read Track. Owning issue: https://github.com/emu198x/emu198x/issues/1942.

The two corrected tests execute 16 cases across stored lengths 0/256, Data/Deleted, SK off/on and logical A aliases 0/2. Every case requires zero payload, exact seven-byte abnormal MA/MD result, idle recovery, exact good-sector next-command payload/result and byte-identical live EDSK.

Explicit Rust 1.99.0: controller, format, +3 common and Spectrum runtime suites passed 572 tests, with 0 failures and 62 ignored across 63 result blocks. Ignored tests remain uncredited firmware prerequisites. Strict all-target Clippy for all four crates and workspace format passed. Exact commands, 600 second process bounds, outputs and identities are retained alongside this plan. No firmware or native-state artifacts were produced.
